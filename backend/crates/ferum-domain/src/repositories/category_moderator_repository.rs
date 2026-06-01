use async_trait::async_trait;
use uuid::Uuid;

use crate::models::category::CategoryModerator;
use crate::AppError;

#[async_trait]
pub trait CategoryModeratorRepository: Send + Sync {
    async fn list_by_category(&self, category_id: Uuid)
        -> Result<Vec<CategoryModerator>, AppError>;
    async fn list_category_ids_for_user(&self, user_id: Uuid) -> Result<Vec<Uuid>, AppError>;
    async fn assign(
        &self,
        category_id: Uuid,
        user_id: Uuid,
        assigned_by_id: Uuid,
    ) -> Result<CategoryModerator, AppError>;
    async fn revoke(&self, category_id: Uuid, user_id: Uuid) -> Result<(), AppError>;
    async fn is_assigned(&self, category_id: Uuid, user_id: Uuid) -> Result<bool, AppError>;
}
