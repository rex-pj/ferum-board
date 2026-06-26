use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::webhook::Webhook;


#[derive(Serialize)]
pub struct WebhookResponse {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub has_secret: bool,
    pub is_active: bool,
    pub failure_count: i32,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Webhook> for WebhookResponse {
    fn from(w: Webhook) -> Self {
        Self {
            id: w.id,
            url: w.url,
            events: w.events,
            has_secret: w.secret.is_some(),
            is_active: w.is_active,
            failure_count: w.failure_count,
            last_triggered_at: w.last_triggered_at,
            created_at: w.created_at,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateWebhookRequest {
    #[validate(url, length(max = 2048))]
    pub url: String,
    pub events: Vec<String>,
    #[validate(length(max = 512))]
    pub secret: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateWebhookRequest {
    #[validate(length(max = 2048))]
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    #[validate(length(max = 512))]
    pub secret: Option<String>,
    pub is_active: Option<bool>,
}
