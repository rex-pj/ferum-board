use chrono::Utc;
use uuid::Uuid;

use crate::shared::AppError;
use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};
use ferum_domain::models::post::Post;
use ferum_domain::models::role::perm;
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
                let u = user.ok_or_else(|| AppError::NotFound)?;
                // has_perm_in checks global + category-scoped — covers both global mods and
                // mods assigned specifically to this staff_only category.
                if u.has_perm_in(perm::MOD_WARN, category.id) || u.has_perm(perm::ADMIN_USERS) {
                    Ok(())
                } else {
                    Err(AppError::NotFound)
                }
            }
        }
    }

    // ─── Post creation ────────────────────────────────────────────────────────

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
        let category_min = match category.post_policy {
            PostPolicy::Members | PostPolicy::Moderated => TrustLevel::Basic,
            PostPolicy::Trusted => TrustLevel::Member,
            PostPolicy::StaffOnly => TrustLevel::Leader,
            PostPolicy::Closed => unreachable!(),
        };
        if !user.has_perm_in(perm::MOD_WARN, category.id) && !user.meets_trust(category_min) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }

        Ok(())
    }

    // ─── Post editing ─────────────────────────────────────────────────────────

    pub fn can_edit_post(user: &AuthUser, post: &Post, category_id: Uuid) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if post.is_deleted {
            return Err(AppError::NotFound);
        }

        // Moderators with post.edit_any in this category can edit any post
        if user.has_perm_in(perm::THREAD_EDIT_ANY, category_id) {
            return Ok(());
        }

        if post.author_id != user.id {
            return Err(AppError::forbidden("not_author"));
        }

        if !user.has_perm(perm::POST_EDIT_OWN) {
            return Err(AppError::forbidden("permission_denied"));
        }

        if !post.is_editable_by_author(Utc::now()) {
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
        if user.has_perm_in(perm::THREAD_PIN, category_id) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_lock(user: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        if user.has_perm_in(perm::THREAD_LOCK, category_id) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_move(user: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        if user.has_perm_in(perm::THREAD_MOVE, category_id) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    // ─── Moderation (cross-category) ─────────────────────────────────────────

    pub fn can_view_reports(user: &AuthUser, category_id: Option<Uuid>) -> Result<(), AppError> {
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
        if user.has_perm(perm::MOD_WARN) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_ban_temp(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::MOD_BAN_TEMP) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_ban_permanent(user: &AuthUser) -> Result<(), AppError> {
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

    // ─── Admin ────────────────────────────────────────────────────────────────

    pub fn can_manage_users(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::ADMIN_USERS) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_categories(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::ADMIN_CATEGORIES) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_roles(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::ADMIN_ROLES) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_config(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::ADMIN_CONFIG) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_webhooks(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::ADMIN_WEBHOOKS) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    pub fn can_manage_plugins(user: &AuthUser) -> Result<(), AppError> {
        if user.has_perm(perm::ADMIN_PLUGINS) {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    pub fn require_not_banned(user: &AuthUser) -> Result<(), AppError> {
        if user.is_currently_banned() {
            Err(AppError::forbidden("account_suspended"))
        } else {
            Ok(())
        }
    }

    pub fn require_auth(user: Option<&AuthUser>) -> Result<&AuthUser, AppError> {
        user.ok_or(AppError::Unauthorized)
    }
}
