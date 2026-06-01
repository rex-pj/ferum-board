use std::sync::Arc;

use uuid::Uuid;

use crate::application::permission::PermissionChecker;
use crate::application::shared::AppError;
use crate::domain::models::notification::Notification;
use crate::domain::repositories::notification_repository::NotificationRepository;
use crate::middleware::auth::AuthUser;

pub struct NotificationUseCase {
    pub notifications: Arc<dyn NotificationRepository>,
}

impl NotificationUseCase {
    pub fn new(notifications: Arc<dyn NotificationRepository>) -> Self {
        Self { notifications }
    }

    pub async fn inbox(
        &self,
        actor: &AuthUser,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Notification>, u64), AppError> {
        PermissionChecker::require_not_banned(actor)?;
        self.notifications.list_for_user(actor.id, page, per_page.min(50)).await
    }

    pub async fn unread_count(&self, actor: &AuthUser) -> Result<u64, AppError> {
        self.notifications.unread_count(actor.id).await
    }

    pub async fn mark_read(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        self.notifications.mark_read(id, actor.id).await
    }

    pub async fn mark_all_read(&self, actor: &AuthUser) -> Result<(), AppError> {
        self.notifications.mark_all_read(actor.id).await
    }
}
