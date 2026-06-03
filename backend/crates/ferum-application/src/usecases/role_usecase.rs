use std::sync::Arc;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::models::role::{Permission, Role, UserRoleAssignment, perm};
use ferum_domain::repositories::permission_repository::PermissionRepository;
use ferum_domain::repositories::role_repository::{NewRole, RoleRepository, UpdateRole};
use ferum_domain::repositories::user_role_repository::UserRoleRepository;
use ferum_domain::AuthUser;

pub struct RoleUseCase {
    pub roles: Arc<dyn RoleRepository>,
    pub permissions: Arc<dyn PermissionRepository>,
    pub user_roles: Arc<dyn UserRoleRepository>,
}

impl RoleUseCase {
    pub fn new(
        roles: Arc<dyn RoleRepository>,
        permissions: Arc<dyn PermissionRepository>,
        user_roles: Arc<dyn UserRoleRepository>,
    ) -> Self {
        Self { roles, permissions, user_roles }
    }

    // ─── Role listing (public — needed for badge display) ─────────────────────

    pub async fn list_roles(&self) -> Result<Vec<Role>, AppError> {
        self.roles.list().await
    }

    // ─── Role CRUD (admin only) ───────────────────────────────────────────────

    pub async fn create_role(
        &self,
        actor: &AuthUser,
        cmd: CreateRoleCmd,
    ) -> Result<Role, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        self.roles
            .create(NewRole {
                slug: cmd.slug,
                name: cmd.name,
                description: cmd.description,
                color: cmd.color,
                position: cmd.position,
            })
            .await
    }

    pub async fn update_role(
        &self,
        actor: &AuthUser,
        id: Uuid,
        cmd: UpdateRoleCmd,
    ) -> Result<Role, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        let _role = self.roles.find_by_id(id).await?.ok_or(AppError::NotFound)?;

        // Prevent renaming system roles' slugs, but allow updating name/color/position
        self.roles
            .update(
                id,
                UpdateRole {
                    name: cmd.name,
                    description: cmd.description,
                    color: cmd.color,
                    position: cmd.position,
                },
            )
            .await
    }

    pub async fn delete_role(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        // Delegates is_system guard to repository
        self.roles.delete(id).await
    }

    // ─── Permission management (admin only) ───────────────────────────────────

    pub async fn get_role_permissions(
        &self,
        actor: &AuthUser,
        role_id: Uuid,
    ) -> Result<Vec<Permission>, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        self.roles.find_by_id(role_id).await?.ok_or(AppError::NotFound)?;
        self.permissions.list_for_role(role_id).await
    }

    pub async fn list_all_permissions(&self, actor: &AuthUser) -> Result<Vec<Permission>, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        self.permissions.list_all().await
    }

    pub async fn set_role_permissions(
        &self,
        actor: &AuthUser,
        role_id: Uuid,
        permission_keys: Vec<String>,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_roles(actor)?;

        let role = self.roles.find_by_id(role_id).await?.ok_or(AppError::NotFound)?;

        // System admin role must always retain admin.roles permission (prevent lock-out)
        if role.slug == "admin" && !permission_keys.contains(&perm::ADMIN_ROLES.to_string()) {
            return Err(AppError::forbidden("cannot_remove_admin_roles_from_admin"));
        }

        self.permissions.set_role_permissions(role_id, &permission_keys).await
    }

    // ─── User-role assignment (admin only) ────────────────────────────────────

    pub async fn list_user_roles(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
    ) -> Result<Vec<UserRoleAssignment>, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.user_roles.list_for_user(user_id).await
    }

    pub async fn assign_role(
        &self,
        actor: &AuthUser,
        cmd: AssignRoleCmd,
    ) -> Result<UserRoleAssignment, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.roles.find_by_id(cmd.role_id).await?.ok_or(AppError::NotFound)?;

        self.user_roles
            .assign(cmd.user_id, cmd.role_id, cmd.category_id, actor.id, cmd.expires_at)
            .await
    }

    pub async fn revoke_role(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.user_roles.revoke(user_id, role_id, category_id).await
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CreateRoleCmd {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub position: i32,
}

#[derive(Debug, Default)]
pub struct UpdateRoleCmd {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub color: Option<Option<String>>,
    pub position: Option<i32>,
}

#[derive(Debug)]
pub struct AssignRoleCmd {
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub category_id: Option<Uuid>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}
