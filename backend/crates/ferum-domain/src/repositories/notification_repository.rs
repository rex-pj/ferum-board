use async_trait::async_trait;
use uuid::Uuid;

use crate::models::notification::{Notification, NotificationKind};
use crate::AppError;

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn list_for_user(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Notification>, u64), AppError>;
    async fn unread_count(&self, user_id: Uuid) -> Result<u64, AppError>;
    async fn create(
        &self,
        user_id: Uuid,
        kind: NotificationKind,
        payload: serde_json::Value,
    ) -> Result<Notification, AppError>;
    async fn mark_read(&self, id: Uuid, user_id: Uuid) -> Result<(), AppError>;
    async fn mark_all_read(&self, user_id: Uuid) -> Result<(), AppError>;
}
