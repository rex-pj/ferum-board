use async_trait::async_trait;
use uuid::Uuid;

use crate::models::role::Permission;
use crate::AppError;

#[async_trait]
pub trait PermissionRepository: Send + Sync {
    async fn list_all(&self) -> Result<Vec<Permission>, AppError>;
    async fn list_for_role(&self, role_id: Uuid) -> Result<Vec<Permission>, AppError>;
    /// Replaces the full permission set for a role.
    /// For the system admin role, caller must ensure admin.roles is always kept.
    async fn set_role_permissions(
        &self,
        role_id: Uuid,
        permission_keys: &[String],
    ) -> Result<(), AppError>;
}
