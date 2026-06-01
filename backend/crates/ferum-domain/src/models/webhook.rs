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
    #[allow(dead_code)]
    pub updated_at: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub created_by_id: Option<Uuid>,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub failure_count: i32,
}
