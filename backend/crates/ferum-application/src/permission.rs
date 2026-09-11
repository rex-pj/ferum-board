//! RBAC + trust + category-policy checks. Every state-changing use case calls
//! one of these BEFORE mutating; middleware only resolves identity.
//!
//! Order matters: ban → permission → trust → category policy. A banned user
//! must never reach a permission check that could grant them something.

use chrono::Utc;
use uuid::Uuid;

use crate::shared::AppError;
use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};
use ferum_domain::models::post::Post;
use ferum_domain::models::role::perm;
use ferum_domain::models::thread::ThreadStatus;
use ferum_domain::models::user::TrustLevel;
use ferum_domain::AuthUser;

pub struct PermissionChecker;

impl PermissionChecker {
    // ─── Category visibility ──────────────────────────────────────────────────

    pub fn can_view_category(user: Option<&AuthUser>, category: &Category) -> Result<(), AppError> {
        match category.view_policy {
            ViewPolicy::Public => Ok(()),
            ViewPolicy::MembersOnly => {
                let u = user.ok_or(AppError::Unauthorized)?;
                if u.has_perm(perm::POST_CREATE) || u.meets_trust(TrustLevel::Basic) {
                    Ok(())
                } else {
                    Err(AppError::forbidden("trust_level_insufficient"))
                }
            }
            ViewPolicy::StaffOnly => {
                let u = user.ok_or(AppError::NotFound)?;
                // MOD_VIEW_REPORTS is the minimum moderation capability — any mod assigned to
                // this category (or globally) should be able to see staff_only categories.
                // MOD_WARN is NOT used here: a mod who can view/resolve reports but cannot
                // warn users is still staff and should see this category.
                if u.has_perm_in(perm::MOD_VIEW_REPORTS, category.id)
                    || u.has_perm(perm::ADMIN_USERS)
                {
                    Ok(())
                } else {
                    Err(AppError::NotFound)
                }
            }
        }
    }

    // ─── Post creation ────────────────────────────────────────────────────────

    /// Bool-returning mirror of `can_create_post`, for gating UI affordances
    /// (e.g. the "New Thread" button) without needing an `AppError` to discard.
    pub fn user_can_create_post(user: Option<&AuthUser>, category: &Category) -> bool {
        match user {
            Some(u) => Self::can_create_post(u, category).is_ok(),
            None => false,
        }
    }

    pub fn can_create_post(user: &AuthUser, category: &Category) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if category.view_policy == ViewPolicy::StaffOnly
            && !user.has_perm_in(perm::MOD_WARN, category.id)
            && !user.has_perm(perm::ADMIN_USERS)
        {
            return Err(AppError::NotFound);
        }

        if category.post_policy == PostPolicy::Closed {
            return Err(AppError::forbidden("category_closed"));
        }

        if !user.has_perm_in(perm::POST_CREATE, category.id) {
            return Err(AppError::forbidden("permission_denied"));
        }

        // Category post_policy provides the trust gate (admin-configurable per category).
        // Staff (users with moderation permissions in this category) bypass the trust gate.
        // Moderated behaves like Members for access; post status is determined after entry.
        //
        // `Closed` returned above; restating it as an arm rather than
        // `unreachable!()` keeps the match exhaustive, so a new `PostPolicy`
        // variant becomes a compile error instead of a panic on a live request.
        let category_min = match category.post_policy {
            PostPolicy::Members | PostPolicy::Moderated => TrustLevel::Basic,
            PostPolicy::Trusted => TrustLevel::Member,
            PostPolicy::StaffOnly => TrustLevel::Leader,
            PostPolicy::Closed => return Err(AppError::forbidden("category_closed")),
        };
        if !user.has_perm_in(perm::MOD_WARN, category.id) && !user.meets_trust(category_min) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }

        Ok(())
    }

    /// Starting a new thread in `category`.
    ///
    /// Everything `can_create_post` enforces, plus `thread.create` — the whole
    /// reason that key exists. Opening a topic and replying are different acts,
    /// and a forum wanting reply-only members cannot say so if they share one
    /// permission. Default roles grant both, so this takes nothing away.
    pub fn can_create_thread(user: &AuthUser, category: &Category) -> Result<(), AppError> {
        Self::can_create_post(user, category)?;

        if !user.has_perm_in(perm::THREAD_CREATE, category.id) {
            return Err(AppError::forbidden("permission_denied"));
        }

        Ok(())
    }

    /// Reacting to a post.
    ///
    /// The trust floor used to be written inline in `ReactionUseCase::add` as a
    /// bare `trust_level < Basic`, which matched `reaction.add`'s declared
    /// min_trust by coincidence rather than by construction, and left the RBAC
    /// half of the permission unchecked. Both halves live here now.
    pub fn can_react(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if !user.has_perm(perm::REACTION_ADD) {
            return Err(AppError::forbidden("permission_denied"));
        }

        if !user.meets_trust(TrustLevel::Basic) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }

        Ok(())
    }

    // ─── Post editing ─────────────────────────────────────────────────────────

    /// `thread_status` is the status of the thread the post lives in.
    ///
    /// It is a parameter rather than something the caller checks separately
    /// because leaving it out is what made locking a thread a half-measure:
    /// `PostUseCase::create` refused on `Locked` and `ThreadUseCase::update_title`
    /// refused on `Locked`, but this did not, so the author of any post in a
    /// locked thread could still rewrite its body. Locking a thread *over its
    /// content* left that content editable by the person who wrote it.
    pub fn can_edit_post(
        user: &AuthUser,
        post: &Post,
        category_id: Uuid,
        thread_status: ThreadStatus,
        edit_window_hours: i64,
    ) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if post.is_deleted || thread_status == ThreadStatus::Deleted {
            return Err(AppError::NotFound);
        }

        // Moderators with post.edit_any in this category can edit any post —
        // including in a locked thread, which is often exactly why they locked
        // it. Same override `update_title` grants.
        if user.has_perm_in(perm::THREAD_EDIT_ANY, category_id) {
            return Ok(());
        }

        if thread_status == ThreadStatus::Locked {
            return Err(AppError::forbidden("thread_locked"));
        }

        if post.author_id != user.id {
            return Err(AppError::forbidden("not_author"));
        }

        if !user.has_perm(perm::POST_EDIT_OWN) {
            return Err(AppError::forbidden("permission_denied"));
        }

        if !post.is_editable_by_author(Utc::now(), edit_window_hours) {
            return Err(AppError::forbidden("edit_window_expired"));
        }

        Ok(())
    }

    // ─── Post deletion ────────────────────────────────────────────────────────

    pub fn can_delete_post(user: &AuthUser, post: &Post, category_id: Uuid) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if post.is_deleted {
            return Err(AppError::NotFound);
        }

        if user.has_perm_in(perm::POST_DELETE_ANY, category_id) {
            return Ok(());
        }

        if post.author_id == user.id && user.has_perm(perm::POST_DELETE_OWN) {
            return Ok(());
        }

        Err(AppError::forbidden("permission_denied"))
    }

    // ─── Thread moderation ────────────────────────────────────────────────────

    pub fn can_pin(user: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm_in(perm::THREAD_PIN, category_id) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_lock(user: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm_in(perm::THREAD_LOCK, category_id) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_move(user: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm_in(perm::THREAD_MOVE, category_id) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    // ─── Moderation (cross-category) ─────────────────────────────────────────

    pub fn can_view_reports(user: &AuthUser, category_id: Option<Uuid>) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        let ok = match category_id {
            Some(cat_id) => user.has_perm_in(perm::MOD_VIEW_REPORTS, cat_id),
            // No specific category — allow if the user holds this permission
            // globally or in ANY of their scoped categories.
            None => user.has_perm_any_category(perm::MOD_VIEW_REPORTS),
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_resolve_report(user: &AuthUser, category_id: Option<Uuid>) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        let ok = match category_id {
            Some(cat_id) => user.has_perm_in(perm::MOD_RESOLVE, cat_id),
            None => user.has_perm_any_category(perm::MOD_RESOLVE),
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_warn(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::MOD_WARN) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_ban_temp(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::MOD_BAN_TEMP) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_ban_permanent(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_BAN_PERMANENT) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    // ─── Upload ───────────────────────────────────────────────────────────────

    pub fn can_upload(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if !user.has_perm(perm::FILE_UPLOAD) {
            return Err(AppError::forbidden("permission_denied"));
        }
        // Default min_trust for file.upload is Member (matches seeded DB value).
        // Staff bypass this gate.
        if !user.has_perm(perm::MOD_WARN) && !user.meets_trust(TrustLevel::Member) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }
        Ok(())
    }

    /// Avatar and cover uploads only require email verification (F-PRF-01, F-PRF-02).
    /// min_trust: Basic — separate from general file.upload (which needs Member).
    pub fn can_upload_profile_image(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if !user.meets_trust(TrustLevel::Basic) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }
        Ok(())
    }

    // ─── Admin ────────────────────────────────────────────────────────────────
    //
    // These eleven checks (six here, five above) once omitted `require_not_banned`,
    // inverting the module rule at the top of this file: ordinary posting was
    // refused for a banned user while every administrative action stayed open, so a
    // banned admin could unban themselves. Middleware resolves the flag but does not
    // reject, and neither `require_admin` nor `require_moderator` looks at it, so
    // there was no second line.
    //
    // The session deliberately stays alive — /account is where a banned user reads
    // why. It is the capability that is withdrawn, not the login.

    pub fn can_manage_users(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_USERS) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_categories(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_CATEGORIES) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_roles(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_ROLES) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_config(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_CONFIG) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    /// Deliberately not `ADMIN_CONFIG`: email copy is written by whoever owns
    /// the site's voice, and that need not be whoever holds the SMTP password
    /// sitting on the same settings page.
    pub fn can_manage_email_templates(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_EMAIL_TEMPLATES) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_webhooks(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_WEBHOOKS) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_plugins(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::ADMIN_PLUGINS) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    // ─── Catalog (furniture review) ───────────────────────────────────────────

    /// Managing the product / material catalog is a curation action. Global
    /// permission — not category-scoped.
    pub fn can_manage_products(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::PRODUCT_MANAGE) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    /// Crowd-sourced contribution: any email-verified member may PROPOSE a
    /// product. It is created as `draft` and only a curator (`product.manage`)
    /// can publish it, so submissions never appear in the public catalog
    /// unreviewed. Curators bypass the trust gate.
    pub fn can_submit_products(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.has_perm(perm::PRODUCT_MANAGE) {
            return Ok(());
        }
        if !user.has_perm(perm::PRODUCT_SUBMIT) {
            return Err(AppError::forbidden("permission_denied"));
        }
        if !user.meets_trust(TrustLevel::Basic) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }
        Ok(())
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    pub fn require_not_banned(user: &AuthUser) -> Result<(), AppError> {
        if user.is_currently_banned() {
            Err(AppError::forbidden("account_suspended"))
        } else {
            Ok(())
        }
    }
}
