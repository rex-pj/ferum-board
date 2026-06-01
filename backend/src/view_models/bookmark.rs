use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::models::bookmark::Bookmark;
use crate::domain::models::thread::Thread;
use crate::view_models::thread::ThreadResponse;

#[derive(Serialize)]
pub struct BookmarkResponse {
    pub id: Uuid,
    pub bookmarked_at: DateTime<Utc>,
    pub thread: ThreadResponse,
}

impl BookmarkResponse {
    pub fn from_pair(bookmark: Bookmark, thread: Thread) -> Self {
        Self {
            id: bookmark.id,
            bookmarked_at: bookmark.created_at,
            thread: ThreadResponse::from(thread),
        }
    }
}

#[derive(Serialize)]
pub struct BookmarkStatusResponse {
    pub bookmarked: bool,
}

#[derive(Debug, Deserialize)]
pub struct BookmarkListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}
