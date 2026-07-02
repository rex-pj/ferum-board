use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::webhook::Webhook;
use ferum_domain::repositories::webhook_repository::{NewWebhook, UpdateWebhook, WebhookRepository};
use ferum_domain::AppError;

mockall::mock! {
    pub WebhookRepository {}

    #[async_trait]
    impl WebhookRepository for WebhookRepository {
        async fn list(&self) -> Result<Vec<Webhook>, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, AppError>;
        async fn find_subscribed(&self, event_type: &str) -> Result<Vec<Webhook>, AppError>;
        async fn create(&self, new: NewWebhook) -> Result<Webhook, AppError>;
        async fn update(&self, id: Uuid, update: UpdateWebhook) -> Result<Webhook, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
        async fn delete_by_plugin(&self, plugin_id: Uuid) -> Result<(), AppError>;
        async fn record_success(&self, id: Uuid) -> Result<(), AppError>;
        async fn record_failure(&self, id: Uuid) -> Result<(), AppError>;
    }
}
