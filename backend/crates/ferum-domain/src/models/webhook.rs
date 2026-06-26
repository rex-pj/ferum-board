use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Webhook {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_by_id: Option<Uuid>,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub failure_count: i32,
    /// Set for Tier 1 plugin-registered webhooks; NULL for user-created webhooks.
    pub plugin_id: Option<Uuid>,
}
