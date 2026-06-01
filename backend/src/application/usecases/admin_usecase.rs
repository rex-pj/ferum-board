use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::application::permission::PermissionChecker;
use crate::application::ports::CacheService;
use crate::application::shared::AppError;
use crate::domain::models::audit_log::AuditLog;
use crate::domain::models::category::{Category, CategoryModerator, PostPolicy, ViewPolicy};
use crate::domain::repositories::audit_log_repository::AuditLogRepository;
use crate::domain::repositories::category_moderator_repository::CategoryModeratorRepository;
use crate::domain::repositories::category_repository::{CategoryRepository, NewCategory, UpdateCategory};
use crate::domain::repositories::user_repository::UserRepository;
use crate::middleware::auth::AuthUser;

pub struct AdminUseCase {
    pub categories: Arc<dyn CategoryRepository>,
    pub category_mods: Arc<dyn CategoryModeratorRepository>,
    pub users: Arc<dyn UserRepository>,
    pub audit_log: Arc<dyn AuditLogRepository>,
    pub cache: Arc<dyn CacheService>,
}

impl AdminUseCase {
    pub fn new(
        categories: Arc<dyn CategoryRepository>,
        category_mods: Arc<dyn CategoryModeratorRepository>,
        users: Arc<dyn UserRepository>,
        audit_log: Arc<dyn AuditLogRepository>,
        cache: Arc<dyn CacheService>,
    ) -> Self {
        Self { categories, category_mods, users, audit_log, cache }
    }

    // ─── Categories ───────────────────────────────────────────────────────────

    pub async fn list_categories(&self, actor: &AuthUser) -> Result<Vec<Category>, AppError> {
        PermissionChecker::can_admin(actor)?;
        self.categories.list_all().await
    }

    pub async fn create_category(
        &self,
        actor: &AuthUser,
        cmd: CreateCategoryCmd,
    ) -> Result<Category, AppError> {
        PermissionChecker::can_admin(actor)?;

        if crate::validators::is_reserved_slug(&cmd.slug) {
            return Err(AppError::unprocessable("slug_reserved"));
        }
        if self.categories.find_by_slug(&cmd.slug).await?.is_some() {
            return Err(AppError::Conflict("slug_taken".to_string()));
        }

        if let Some(parent_id) = cmd.parent_id {
            let parent = self
                .categories
                .find_by_id(parent_id)
                .await?
                .ok_or(AppError::NotFound)?;
            if parent.parent_id.is_some() {
                return Err(AppError::unprocessable("Max 2 levels of category nesting"));
            }
        }

        self.categories
            .create(NewCategory {
                slug: cmd.slug,
                name: cmd.name,
                description: cmd.description,
                parent_id: cmd.parent_id,
                position: cmd.position,
                view_policy: cmd.view_policy,
                post_policy: cmd.post_policy,
                color: cmd.color,
                created_by_id: Some(actor.id),
            })
            .await
    }

    pub async fn update_category(
        &self,
        actor: &AuthUser,
        id: Uuid,
        cmd: UpdateCategoryCmd,
    ) -> Result<Category, AppError> {
        PermissionChecker::can_admin(actor)?;

        self.categories.find_by_id(id).await?.ok_or(AppError::NotFound)?;

        if let Some(ref slug) = cmd.slug {
            if crate::validators::is_reserved_slug(slug) {
                return Err(AppError::unprocessable("slug_reserved"));
            }
            if let Some(existing) = self.categories.find_by_slug(slug).await? {
                if existing.id != id {
                    return Err(AppError::Conflict("slug_taken".to_string()));
                }
            }
        }

        self.categories
            .update(
                id,
                UpdateCategory {
                    name: cmd.name,
                    slug: cmd.slug,
                    description: cmd.description,
                    parent_id: cmd.parent_id,
                    position: cmd.position,
                    view_policy: cmd.view_policy,
                    post_policy: cmd.post_policy,
                    color: cmd.color,
                    updated_by_id: Some(actor.id),
                },
            )
            .await
    }

    pub async fn delete_category(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_admin(actor)?;
        self.categories.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.categories.delete(id).await
    }

    // ─── Moderator assignment ─────────────────────────────────────────────────

    pub async fn list_category_moderators_with_users(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
    ) -> Result<Vec<(CategoryModerator, crate::domain::models::user::User)>, AppError> {
        PermissionChecker::can_admin(actor)?;
        self.categories.find_by_id(category_id).await?.ok_or(AppError::NotFound)?;
        let mods = self.category_mods.list_by_category(category_id).await?;

        let user_ids: Vec<Uuid> = mods.iter().map(|m| m.user_id).collect();
        let users = self.users.find_many_by_ids(&user_ids).await?;
        let user_map: std::collections::HashMap<Uuid, _> =
            users.into_iter().map(|u| (u.id, u)).collect();

        mods.into_iter()
            .map(|m| {
                let user = user_map.get(&m.user_id).cloned().ok_or(AppError::NotFound)?;
                Ok((m, user))
            })
            .collect()
    }

    pub async fn assign_moderator(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
        user_id: Uuid,
    ) -> Result<CategoryModerator, AppError> {
        PermissionChecker::can_admin(actor)?;
        self.categories.find_by_id(category_id).await?.ok_or(AppError::NotFound)?;

        let user = self.users.find_by_id(user_id).await?.ok_or(AppError::NotFound)?;
        if !user.role.is_staff() {
            return Err(AppError::unprocessable(
                "User must have role moderator or admin to be assigned as category moderator",
            ));
        }

        if self.category_mods.is_assigned(category_id, user_id).await? {
            return Err(AppError::Conflict("already_assigned".to_string()));
        }

        self.category_mods.assign(category_id, user_id, actor.id).await
    }

    pub async fn revoke_moderator(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        PermissionChecker::can_admin(actor)?;
        self.category_mods.revoke(category_id, user_id).await
    }

    // ─── User management ──────────────────────────────────────────────────────

    pub async fn list_users(
        &self,
        actor: &AuthUser,
        page: u64,
        per_page: u64,
        search: Option<&str>,
    ) -> Result<(Vec<crate::domain::models::user::User>, u64), AppError> {
        PermissionChecker::can_admin(actor)?;
        self.users.list_paginated(page, per_page, search).await
    }

    pub async fn get_user(&self, actor: &AuthUser, id: Uuid) -> Result<crate::domain::models::user::User, AppError> {
        PermissionChecker::can_admin(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

    pub async fn update_user_role(
        &self,
        actor: &AuthUser,
        id: Uuid,
        cmd: AdminUpdateUserCmd,
    ) -> Result<crate::domain::models::user::User, AppError> {
        PermissionChecker::can_admin(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        let updated = self.users.update(id, crate::domain::repositories::user_repository::UpdateUser {
            role: cmd.role,
            is_global_mod: cmd.is_global_mod,
            ..Default::default()
        }).await?;
        self.audit_log
            .append(AuditLog::user_action(
                actor.id,
                "user.role_changed",
                "user",
                id,
                Some(serde_json::json!({
                    "new_role": cmd.role.map(|r| format!("{:?}", r).to_lowercase()),
                    "new_is_global_mod": cmd.is_global_mod,
                })),
            ))
            .await
            .ok();
        Ok(updated)
    }

    pub async fn permanent_ban(&self, actor: &AuthUser, id: Uuid, reason: String) -> Result<(), AppError> {
        PermissionChecker::can_admin(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.users.update(id, crate::domain::repositories::user_repository::UpdateUser {
            is_banned: Some(true),
            banned_until: Some(None),
            ban_reason: Some(Some(reason)),
            ..Default::default()
        }).await?;
        // Immediate enforcement: block all existing access tokens and revoke
        // all refresh tokens so the user cannot re-authenticate.
        self.cache
            .set(&format!("user:banned:{}", id), "1", Duration::from_secs(365 * 24 * 3600))
            .await
            .ok();
        self.cache
            .del_prefix(&format!("refresh:{}:", id))
            .await
            .ok();
        Ok(())
    }

    pub async fn unban(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_admin(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.users.update(id, crate::domain::repositories::user_repository::UpdateUser {
            is_banned: Some(false),
            banned_until: Some(None),
            ban_reason: Some(None),
            ..Default::default()
        }).await?;
        self.cache.del(&format!("user:banned:{}", id)).await.ok();
        Ok(())
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct AdminUpdateUserCmd {
    pub role: Option<crate::domain::models::user::UserRole>,
    pub is_global_mod: Option<bool>,
}

#[derive(Debug)]
pub struct CreateCategoryCmd {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub view_policy: ViewPolicy,
    pub post_policy: PostPolicy,
    pub color: Option<String>,
}

#[derive(Debug, Default)]
pub struct UpdateCategoryCmd {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<Option<String>>,
    pub parent_id: Option<Option<Uuid>>,
    pub position: Option<i32>,
    pub view_policy: Option<ViewPolicy>,
    pub post_policy: Option<PostPolicy>,
    pub color: Option<Option<String>>,
}
