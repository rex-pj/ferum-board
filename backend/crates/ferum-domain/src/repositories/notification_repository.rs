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

    /// Deletes notifications past their retention window, returning the count.
    ///
    /// Two windows: a read notification has done its job, an unread one is still
    /// pending and gets far longer. Without this the table grows for the life of
    /// the account and `/notifications` costs a `COUNT` over all of it.
    async fn delete_expired(
        &self,
        read_retention_days: u32,
        unread_retention_days: u32,
    ) -> Result<u64, AppError>;
}
