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

    /// Deletes notifications past their retention window, returning how many went.
    ///
    /// Two windows because read and unread carry different value. A read
    /// notification has already done its job — the user saw it, and the thread
    /// or post it points at is still there to find. An unread one is a pending
    /// item the user has never seen, so discarding it loses information; it gets
    /// a much longer grace period and is removed only once it is old enough that
    /// nobody is coming back for it.
    ///
    /// Without this the table grows for the life of the account, and the
    /// inbox's `COUNT` grows with it — the cost of opening `/notifications`
    /// would be a function of how long you have had the account.
    async fn delete_expired(
        &self,
        read_retention_days: u32,
        unread_retention_days: u32,
    ) -> Result<u64, AppError>;
}
