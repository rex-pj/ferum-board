use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use ferum_domain::models::role::UserRoleAssignment;
use ferum_domain::repositories::user_role_repository::UserRoleRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub UserRoleRepository {}

    #[async_trait]
    impl UserRoleRepository for UserRoleRepository {
        async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<UserRoleAssignment>, AppError>;
        async fn list_for_users(&self, user_ids: &[Uuid]) -> Result<Vec<UserRoleAssignment>, AppError>;
        async fn list_for_category(&self, role_id: Uuid, category_id: Option<Uuid>) -> Result<Vec<UserRoleAssignment>, AppError>;
        async fn assign(&self, user_id: Uuid, role_id: Uuid, category_id: Option<Uuid>, granted_by: Uuid, expires_at: Option<DateTime<Utc>>) -> Result<UserRoleAssignment, AppError>;
        async fn revoke(&self, user_id: Uuid, role_id: Uuid, category_id: Option<Uuid>) -> Result<(), AppError>;
        async fn count_by_role(&self) -> Result<HashMap<Uuid, u64>, AppError>;
        async fn count_global_by_role(&self) -> Result<HashMap<Uuid, u64>, AppError>;
    }
}
