use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: Uuid,
    pub category_id: Uuid,
    pub category_slug: String,
    pub category_name: Option<String>,
    pub author_id: Uuid,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub title: String,
    pub slug: String,
    pub status: ThreadStatus,
    pub is_pinned: bool,
    pub is_solved: bool,
    pub best_answer_id: Option<Uuid>,
    pub view_count: i32,
    pub reply_count: i32,
    pub last_post_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_by_id: Option<Uuid>,
    pub excerpt: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum ThreadStatus {
    Open,
    Locked,
    Deleted,
}
