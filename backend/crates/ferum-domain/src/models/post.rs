use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::reaction::ReactionKind;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    Pending,
    Published,
}

impl PostStatus {
    pub fn is_pending(&self) -> bool {
        matches!(self, PostStatus::Pending)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
    pub content_html: String,
    pub status: PostStatus,
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
    // Populated only by list_by_author for the user posts page
    pub thread_slug: Option<String>,
    pub thread_title: Option<String>,
}

impl Post {
    /// The `(markdown, html)` a **reader** may see — empty for a deleted post.
    ///
    /// Deleting a post is a soft delete, so the row survives and still carries
    /// its text: a moderator has to be able to review what was removed, and
    /// `sync_attachment_refs` reads `content_md` back to release the
    /// attachments the post referenced. Redaction is therefore a read-time
    /// rule, and it has to be applied by *every* reader path.
    ///
    /// It was not. The SSR thread handler blanked the content inline; the JSON
    /// API had no equivalent, so `PostResponse` copied both fields straight out
    /// of the row and `GET /api/threads/{id}/posts` returned the full text of
    /// every deleted post to anyone, signed in or not. "Delete my post" hid it
    /// from the page and from nothing else.
    ///
    /// One definition here so a third reader cannot repeat the omission. The
    /// row itself is never mutated.
    pub fn readable_content(&self) -> (&str, &str) {
        if self.is_deleted {
            ("", "")
        } else {
            (&self.content_md, &self.content_html)
        }
    }

    /// `edit_window_hours` comes from site_config (`post_edit_window_hours`) —
    /// the caller resolves it so admin changes apply to posts and threads alike.
    pub fn is_editable_by_author(&self, now: DateTime<Utc>, edit_window_hours: i64) -> bool {
        if self.is_deleted {
            return false;
        }
        now - self.created_at <= chrono::Duration::hours(edit_window_hours)
    }
}
