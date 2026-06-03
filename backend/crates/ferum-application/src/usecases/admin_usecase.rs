use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::ports::CacheService;
use crate::shared::AppError;
use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};
use ferum_domain::models::role::UserRoleAssignment;
use ferum_domain::models::user::User;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_domain::repositories::category_repository::{
    CategoryRepository, NewCategory, UpdateCategory,
};
use ferum_domain::repositories::role_repository::RoleRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::repositories::user_role_repository::UserRoleRepository;
use ferum_domain::AuthUser;

pub struct AdminUseCase {
    pub categories: Arc<dyn CategoryRepository>,
    pub roles: Arc<dyn RoleRepository>,
    pub user_roles: Arc<dyn UserRoleRepository>,
    pub users: Arc<dyn UserRepository>,
    pub audit_log: Arc<dyn AuditLogRepository>,
    pub cache: Arc<dyn CacheService>,
}

impl AdminUseCase {
    pub fn new(
        categories: Arc<dyn CategoryRepository>,
        roles: Arc<dyn RoleRepository>,
        user_roles: Arc<dyn UserRoleRepository>,
        users: Arc<dyn UserRepository>,
        audit_log: Arc<dyn AuditLogRepository>,
        cache: Arc<dyn CacheService>,
    ) -> Self {
        Self { categories, roles, user_roles, users, audit_log, cache }
    }

    // ─── Categories ───────────────────────────────────────────────────────────

    pub async fn list_categories(&self, actor: &AuthUser) -> Result<Vec<Category>, AppError> {
        PermissionChecker::can_manage_categories(actor)?;
        self.categories.list_all().await
    }

    pub async fn create_category(
        &self,
        actor: &AuthUser,
        cmd: CreateCategoryCmd,
    ) -> Result<Category, AppError> {
        PermissionChecker::can_manage_categories(actor)?;

        if crate::validators::is_reserved_slug(&cmd.slug) {
            return Err(AppError::unprocessable("slug_reserved"));
        }
        if self.categories.find_by_slug(&cmd.slug).await?.is_some() {
            return Err(AppError::Conflict("slug_taken".to_string()));
        }

        if let Some(parent_id) = cmd.parent_id {
            let parent = self.categories.find_by_id(parent_id).await?.ok_or(AppError::NotFound)?;
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
        PermissionChecker::can_manage_categories(actor)?;

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
        PermissionChecker::can_manage_categories(actor)?;
        self.categories.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.categories.delete(id).await
    }

    // ─── Category-scoped moderator assignment (via user_roles) ────────────────

    pub async fn list_category_moderators(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
    ) -> Result<Vec<(UserRoleAssignment, User)>, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.categories.find_by_id(category_id).await?.ok_or(AppError::NotFound)?;

        let mod_role = self
            .roles
            .find_by_slug("moderator")
            .await?
            .ok_or_else(|| AppError::internal("moderator role not found".to_string()))?;

        let assignments = self.user_roles.list_for_category(mod_role.id, Some(category_id)).await?;

        let user_ids: Vec<Uuid> = assignments.iter().map(|a| a.user_id).collect();
        let users = self.users.find_many_by_ids(&user_ids).await?;
        let user_map: std::collections::HashMap<Uuid, User> =
            users.into_iter().map(|u| (u.id, u)).collect();

        assignments
            .into_iter()
            .map(|a| {
                let user = user_map.get(&a.user_id).cloned().ok_or(AppError::NotFound)?;
                Ok((a, user))
            })
            .collect()
    }

    pub async fn assign_moderator(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
        user_id: Uuid,
    ) -> Result<UserRoleAssignment, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.categories.find_by_id(category_id).await?.ok_or(AppError::NotFound)?;
        self.users.find_by_id(user_id).await?.ok_or(AppError::NotFound)?;

        let mod_role = self
            .roles
            .find_by_slug("moderator")
            .await?
            .ok_or_else(|| AppError::internal("moderator role not found".to_string()))?;

        self.user_roles.assign(user_id, mod_role.id, Some(category_id), actor.id, None).await
    }

    pub async fn revoke_moderator(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        let mod_role = self
            .roles
            .find_by_slug("moderator")
            .await?
            .ok_or_else(|| AppError::internal("moderator role not found".to_string()))?;

        self.user_roles.revoke(user_id, mod_role.id, Some(category_id)).await?;
        // Invalidate user roles cache
        self.cache.del(&format!("user:roles:{}", user_id)).await.ok();
        Ok(())
    }

    // ─── User management ──────────────────────────────────────────────────────

    pub async fn list_users(
        &self,
        actor: &AuthUser,
        page: u64,
        per_page: u64,
        search: Option<&str>,
    ) -> Result<(Vec<User>, u64), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.users.list_paginated(page, per_page, search).await
    }

    pub async fn get_user(&self, actor: &AuthUser, id: Uuid) -> Result<User, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

    pub async fn permanent_ban(
        &self,
        actor: &AuthUser,
        id: Uuid,
        reason: String,
    ) -> Result<(), AppError> {
        PermissionChecker::can_ban_permanent(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.users
            .update(
                id,
                ferum_domain::repositories::user_repository::UpdateUser {
                    is_banned: Some(true),
                    banned_until: Some(None),
                    ban_reason: Some(Some(reason)),
                    ..Default::default()
                },
            )
            .await?;
        self.cache
            .set(&format!("user:banned:{}", id), "1", Duration::from_secs(365 * 24 * 3600))
            .await
            .ok();
        self.cache.del_prefix(&format!("refresh:{}:", id)).await.ok();
        self.cache.del(&format!("user:roles:{}", id)).await.ok();
        Ok(())
    }

    pub async fn unban(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_ban_permanent(actor)?;
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.users
            .update(
                id,
                ferum_domain::repositories::user_repository::UpdateUser {
                    is_banned: Some(false),
                    banned_until: Some(None),
                    ban_reason: Some(None),
                    ..Default::default()
                },
            )
            .await?;
        self.cache.del(&format!("user:banned:{}", id)).await.ok();
        Ok(())
    }

    pub async fn list_audit_log(
        &self,
        actor: &AuthUser,
        actor_id: Option<Uuid>,
        target_type: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<AuditLog>, u64), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.audit_log.list(actor_id, target_type, page, per_page.min(50)).await
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

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
