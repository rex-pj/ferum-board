use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::role::UserRoleAssignment;
use crate::AppError;

#[async_trait]
pub trait UserRoleRepository: Send + Sync {
    /// All active (non-expired) role assignments for a user.
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<UserRoleAssignment>, AppError>;

    /// All users assigned a specific role in a specific category (or globally if category_id is None).
    async fn list_for_category(
        &self,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<Vec<UserRoleAssignment>, AppError>;

    async fn assign(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
        granted_by: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<UserRoleAssignment, AppError>;

    async fn revoke(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    async fn is_assigned(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<bool, AppError>;
}
