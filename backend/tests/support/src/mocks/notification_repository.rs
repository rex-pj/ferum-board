use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::notification::{Notification, NotificationKind};
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub NotificationRepository {}

    #[async_trait]
    impl NotificationRepository for NotificationRepository {
        async fn list_for_user(&self, user_id: Uuid, page: u64, per_page: u64) -> Result<(Vec<Notification>, u64), AppError>;
        async fn unread_count(&self, user_id: Uuid) -> Result<u64, AppError>;
        async fn create(&self, user_id: Uuid, kind: NotificationKind, payload: serde_json::Value) -> Result<Notification, AppError>;
        async fn mark_read(&self, id: Uuid, user_id: Uuid) -> Result<(), AppError>;
        async fn mark_all_read(&self, user_id: Uuid) -> Result<(), AppError>;
    }
}

pub struct NoopNotificationRepository;

#[async_trait]
impl NotificationRepository for NoopNotificationRepository {
    async fn list_for_user(&self, _user_id: Uuid, _page: u64, _per_page: u64) -> Result<(Vec<Notification>, u64), AppError> { Ok((vec![], 0)) }
    async fn unread_count(&self, _user_id: Uuid) -> Result<u64, AppError> { Ok(0) }
    async fn create(&self, user_id: Uuid, kind: NotificationKind, payload: serde_json::Value) -> Result<Notification, AppError> {
        Ok(Notification { id: Uuid::new_v4(), user_id, kind, payload, is_read: false, read_at: None, created_at: chrono::Utc::now() })
    }
    async fn mark_read(&self, _id: Uuid, _user_id: Uuid) -> Result<(), AppError> { Ok(()) }
    async fn mark_all_read(&self, _user_id: Uuid) -> Result<(), AppError> { Ok(()) }
}
