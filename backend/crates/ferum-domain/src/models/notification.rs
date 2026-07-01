use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: NotificationKind,
    pub payload: serde_json::Value,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum NotificationKind {
    Reply,
    Mention,
    Reaction,
    BestAnswer,
    Warn,
    System,
}

impl NotificationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationKind::Reply => "reply",
            NotificationKind::Mention => "mention",
            NotificationKind::Reaction => "reaction",
            NotificationKind::BestAnswer => "best_answer",
            NotificationKind::Warn => "warn",
            NotificationKind::System => "system",
        }
    }
}
