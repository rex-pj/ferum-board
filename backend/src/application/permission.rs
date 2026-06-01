#![allow(dead_code)]

use chrono::Utc;
use uuid::Uuid;

use crate::application::shared::AppError;
use crate::domain::models::category::{Category, PostPolicy, ViewPolicy};
use crate::domain::models::post::Post;
use crate::domain::models::user::{TrustLevel, UserRole};
use crate::middleware::auth::AuthUser;

pub struct PermissionChecker;

impl PermissionChecker {
    // ─── Category visibility ──────────────────────────────────────────────────

    pub fn can_view_category(
        user: Option<&AuthUser>,
        category: &Category,
    ) -> Result<(), AppError> {
        match category.view_policy {
            ViewPolicy::Public => Ok(()),
            ViewPolicy::MembersOnly => {
                let u = user.ok_or(AppError::Unauthorized)?;
                if u.role >= UserRole::Moderator || u.trust_level >= TrustLevel::Basic {
                    Ok(())
                } else {
                    Err(AppError::forbidden("trust_level_insufficient"))
                }
            }
            ViewPolicy::StaffOnly => {
                let u = user.ok_or_else(|| AppError::NotFound)?;
                if u.role >= UserRole::Moderator {
                    Ok(())
                } else {
                    Err(AppError::NotFound)
                }
            }
        }
    }

    // ─── Post creation ────────────────────────────────────────────────────────

    pub fn can_create_post(user: &AuthUser, category: &Category) -> Result<(), AppError> {
        Self::require_role(user, UserRole::Member)?;
        Self::require_not_banned(user)?;

        if category.view_policy == ViewPolicy::StaffOnly && user.role < UserRole::Moderator {
            return Err(AppError::NotFound);
        }

        let min_trust = match category.post_policy {
            PostPolicy::Members => TrustLevel::Basic,
            PostPolicy::Trusted => TrustLevel::Member,
            PostPolicy::StaffOnly => TrustLevel::Leader,
            PostPolicy::Closed => return Err(AppError::forbidden("category_closed")),
        };

        if user.role < UserRole::Moderator && user.trust_level < min_trust {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }

        Ok(())
    }

    // ─── Post editing ─────────────────────────────────────────────────────────

    pub fn can_edit_post(user: &AuthUser, post: &Post) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if post.is_deleted {
            return Err(AppError::NotFound);
        }

        if user.role >= UserRole::Moderator {
            return Ok(());
        }

        if post.author_id != user.id {
            return Err(AppError::forbidden("not_author"));
        }

        if !post.is_editable_by_author(Utc::now()) {
            return Err(AppError::forbidden("edit_window_expired"));
        }

        Ok(())
    }

    // ─── Post deletion ────────────────────────────────────────────────────────

    pub fn can_delete_post(user: &AuthUser, post: &Post) -> Result<(), AppError> {
        Self::require_not_banned(user)?;

        if post.is_deleted {
            return Err(AppError::NotFound);
        }

        if user.role >= UserRole::Moderator || post.author_id == user.id {
            return Ok(());
        }

        Err(AppError::forbidden("not_author"))
    }

    // ─── Moderation ───────────────────────────────────────────────────────────

    pub fn can_moderate(
        user: &AuthUser,
        category_id: Uuid,
        assigned_category_ids: &[Uuid],
    ) -> Result<(), AppError> {
        if user.role == UserRole::Admin {
            return Ok(());
        }
        if user.role == UserRole::Moderator
            && (user.is_global_mod || assigned_category_ids.contains(&category_id))
        {
            return Ok(());
        }
        Err(AppError::forbidden("not_moderator_of_category"))
    }

    /// True for global moderators and admins regardless of category.
    /// Use this when the operation isn't scoped to a specific category (e.g.
    /// viewing the cross-category report queue).
    pub fn can_moderate_any(user: &AuthUser) -> Result<(), AppError> {
        if user.role == UserRole::Admin
            || (user.role == UserRole::Moderator && user.is_global_mod)
        {
            return Ok(());
        }
        Err(AppError::forbidden("not_global_moderator_or_admin"))
    }

    // ─── Admin ────────────────────────────────────────────────────────────────

    pub fn can_admin(user: &AuthUser) -> Result<(), AppError> {
        if user.role == UserRole::Admin {
            Ok(())
        } else {
            Err(AppError::forbidden("not_admin"))
        }
    }

    // ─── Upload / link ────────────────────────────────────────────────────────

    pub fn can_upload(user: &AuthUser) -> Result<(), AppError> {
        Self::require_not_banned(user)?;
        if user.role >= UserRole::Moderator || user.trust_level >= TrustLevel::Member {
            Ok(())
        } else {
            Err(AppError::forbidden("trust_level_insufficient"))
        }
    }

    // ─── Private helpers ──────────────────────────────────────────────────────

    pub fn require_role(user: &AuthUser, min: UserRole) -> Result<(), AppError> {
        if user.role >= min {
            Ok(())
        } else {
            Err(AppError::forbidden("insufficient_role"))
        }
    }

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
