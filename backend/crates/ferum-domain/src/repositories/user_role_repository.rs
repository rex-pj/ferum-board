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

    /// All active (non-expired) role assignments for a batch of users (one query, no N+1).
    async fn list_for_users(&self, user_ids: &[Uuid]) -> Result<Vec<UserRoleAssignment>, AppError>;

    /// Count of non-expired assignments per role (global + category-scoped, deduplicated per user).
    async fn count_by_role(&self) -> Result<std::collections::HashMap<Uuid, u64>, AppError>;

    /// Count of non-expired GLOBAL (category_id IS NULL) assignments per role, deduplicated per user.
    /// Used to guard last-admin revocation so a category-scoped admin cannot inflate the count.
    async fn count_global_by_role(&self) -> Result<std::collections::HashMap<Uuid, u64>, AppError>;
}
