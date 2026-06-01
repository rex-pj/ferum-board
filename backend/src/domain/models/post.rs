use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::models::reaction::ReactionKind;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
    pub content_html: String,
    pub is_deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_by_id: Option<Uuid>,
    pub edited_at: Option<DateTime<Utc>>,
    pub edited_by_id: Option<Uuid>,
    pub edit_count: i32,
    pub created_at: DateTime<Utc>,
    // Enriched fields — populated by PostUseCase.list_by_thread, empty elsewhere
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub author_role: Option<String>,
    pub reactions: Vec<(ReactionKind, u64)>,
    pub my_reactions: Vec<ReactionKind>,
}

impl Post {
    pub fn is_editable_by_author(&self, now: DateTime<Utc>) -> bool {
        if self.is_deleted {
            return false;
        }
        let edit_window = chrono::Duration::hours(crate::constants::POST_EDIT_WINDOW_HOURS);
        now - self.created_at <= edit_window
    }
}
