use async_trait::async_trait;
use uuid::Uuid;

use crate::models::webhook::Webhook;
use crate::AppError;

#[derive(Debug)]
pub struct NewWebhook {
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub created_by_id: Option<Uuid>,
    /// Set to `Some(plugin_id)` when registering webhooks on behalf of a Tier 1 plugin.
    pub plugin_id: Option<Uuid>,
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
    /// Delete all webhooks whose `plugin_id` matches. Called on plugin deactivation.
    async fn delete_by_plugin(&self, plugin_id: Uuid) -> Result<(), AppError>;
    /// Called on successful delivery: update last_triggered_at, reset failure_count to 0.
    async fn record_success(&self, id: Uuid) -> Result<(), AppError>;
    /// Called on failed delivery: increment failure_count; auto-disable when >= WEBHOOK_MAX_FAILURES.
    async fn record_failure(&self, id: Uuid) -> Result<(), AppError>;
}
