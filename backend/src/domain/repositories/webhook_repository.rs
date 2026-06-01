use async_trait::async_trait;
use uuid::Uuid;

use crate::application::shared::AppError;
use crate::domain::models::webhook::Webhook;

#[derive(Debug)]
pub struct NewWebhook {
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub created_by_id: Option<Uuid>,
}

#[derive(Debug)]
pub struct UpdateWebhook {
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub secret: Option<String>,
    pub is_active: Option<bool>,
}

#[async_trait]
pub trait WebhookRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Webhook>, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, AppError>;
    async fn find_subscribed(&self, event_type: &str) -> Result<Vec<Webhook>, AppError>;
    async fn create(&self, new: NewWebhook) -> Result<Webhook, AppError>;
    async fn update(&self, id: Uuid, update: UpdateWebhook) -> Result<Webhook, AppError>;
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;
    /// Called on successful delivery: update last_triggered_at, reset failure_count to 0.
    async fn record_success(&self, id: Uuid) -> Result<(), AppError>;
    /// Called on failed delivery: increment failure_count; auto-disable when >= WEBHOOK_MAX_FAILURES.
    async fn record_failure(&self, id: Uuid) -> Result<(), AppError>;
}
