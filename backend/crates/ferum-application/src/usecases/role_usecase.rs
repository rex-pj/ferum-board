use std::sync::Arc;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::shared::{AppError, OptionExt};
use ferum_domain::models::role::{Permission, Role, UserRoleAssignment};
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

    #[tracing::instrument(skip(self))]
    pub async fn list_roles(&self) -> Result<Vec<Role>, AppError> {
        self.roles.list().await
    }

    // ─── Role CRUD (admin only) ───────────────────────────────────────────────

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id))]
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

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id, role_id = %id))]
    pub async fn update_role(
        &self,
        actor: &AuthUser,
        id: Uuid,
        cmd: UpdateRoleCmd,
    ) -> Result<Role, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        let _role = self.roles.find_by_id(id).await?.or_not_found()?;

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

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, role_id = %id))]
    pub async fn delete_role(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        // Delegates is_system guard to repository
        self.roles.delete(id).await
    }

    // ─── Permission management (admin only) ───────────────────────────────────

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, role_id = %role_id))]
    pub async fn get_role_permissions(
        &self,
        actor: &AuthUser,
        role_id: Uuid,
    ) -> Result<Vec<Permission>, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        self.roles.find_by_id(role_id).await?.or_not_found()?;
        self.permissions.list_for_role(role_id).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn list_all_permissions(&self, actor: &AuthUser) -> Result<Vec<Permission>, AppError> {
        PermissionChecker::can_manage_roles(actor)?;
        self.permissions.list_all().await
    }

    #[tracing::instrument(skip(self, actor, permission_keys), fields(user_id = %actor.id, role_id = %role_id))]
    pub async fn set_role_permissions(
        &self,
        actor: &AuthUser,
        role_id: Uuid,
        permission_keys: Vec<String>,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_roles(actor)?;

        let role = self.roles.find_by_id(role_id).await?.or_not_found()?;

        // System roles (admin, moderator, member) cannot have their permissions changed via the
        // API. Their permission sets are defined by the seed migration and are the baseline of
        // the RBAC model. Allowing arbitrary mutation would let an attacker with admin.roles
        // escalate all members to admin or strip all moderation capability from the moderator role.
        if role.is_system {
            return Err(AppError::forbidden("cannot_modify_system_role_permissions"));
        }

        self.permissions.set_role_permissions(role_id, &permission_keys).await
    }

    // ─── User-role assignment (admin only) ────────────────────────────────────

    /// Returns a map of role_id → member count (all scopes, non-expired, distinct users).
    #[tracing::instrument(skip(self))]
    pub async fn count_members_by_role(
        &self,
    ) -> Result<std::collections::HashMap<Uuid, u64>, AppError> {
        // All scopes (global + category-scoped) for the member count badge — intentional.
        self.user_roles.count_by_role().await
    }

    /// Returns all non-expired role assignments for a batch of users (one query, no N+1).
    #[tracing::instrument(skip(self, user_ids), fields(count = user_ids.len()))]
    pub async fn list_role_assignments_for_users(
        &self,
        user_ids: &[Uuid],
    ) -> Result<Vec<UserRoleAssignment>, AppError> {
        self.user_roles.list_for_users(user_ids).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, target_user_id = %user_id))]
    pub async fn list_user_roles(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
    ) -> Result<Vec<UserRoleAssignment>, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.user_roles.list_for_user(user_id).await
    }

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id, target_user_id = %cmd.user_id, role_id = %cmd.role_id))]
    pub async fn assign_role(
        &self,
        actor: &AuthUser,
        cmd: AssignRoleCmd,
    ) -> Result<UserRoleAssignment, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.roles.find_by_id(cmd.role_id).await?.or_not_found()?;

        // Prevent privilege escalation: an actor can only grant a role that carries
        // permissions they already hold themselves (in the same scope). Without this,
        // any actor with just `admin.users` could assign a role bundling `admin.roles`,
        // `admin.config`, etc. to themselves or others and fully escalate.
        let role_perms = self.permissions.list_for_role(cmd.role_id).await?;
        let actor_has_all = role_perms.iter().all(|p| match cmd.category_id {
            Some(category_id) => actor.has_perm(&p.key) || actor.has_perm_in(&p.key, category_id),
            None => actor.has_perm(&p.key),
        });
        if !actor_has_all {
            return Err(AppError::forbidden("cannot_grant_permissions_you_lack"));
        }

        self.user_roles
            .assign(cmd.user_id, cmd.role_id, cmd.category_id, actor.id, cmd.expires_at)
            .await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, target_user_id = %user_id, role_id = %role_id))]
    pub async fn revoke_role(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_users(actor)?;

        // Prevent removing the admin role when this is the last global admin assignment.
        // A category-scoped assignment of the admin role is not a system admin, so we only
        // guard global (category_id == None) revocations of the admin role slug.
        if category_id.is_none() {
            if let Some(role) = self.roles.find_by_id(role_id).await? {
                if role.slug == "admin" {
                    let counts = self.user_roles.count_global_by_role().await?;
                    let global_admin_count = counts.get(&role_id).copied().unwrap_or(0);
                    if global_admin_count <= 1 {
                        return Err(AppError::forbidden("cannot_remove_last_admin"));
                    }
                }
            }
        }

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
