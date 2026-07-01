use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use ferum_domain::models::notification::Notification;

#[derive(Serialize)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Notification> for NotificationResponse {
    fn from(n: Notification) -> Self {
        Self {
            id: n.id,
            user_id: n.user_id,
            kind: n.kind.as_str().to_string(),
            payload: n.payload,
            is_read: n.is_read,
            read_at: n.read_at,
            created_at: n.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct UnreadCountResponse {
    pub count: u64,
}
