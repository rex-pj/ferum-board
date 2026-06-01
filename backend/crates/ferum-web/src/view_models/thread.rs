use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_domain::models::thread::Thread;

// ─── Requests ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MoveThreadRequest {
    pub category_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct MarkSolvedRequest {
    pub best_answer_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct ThreadListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

// ─── Nested response types ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ThreadAuthorResponse {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Serialize)]
pub struct ThreadCategoryResponse {
    pub slug: String,
    pub name: String,
}

// ─── Thread Response ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ThreadResponse {
    pub id: Uuid,
    pub category_id: Uuid,
    pub category_slug: String,
    pub author_id: Uuid,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub is_pinned: bool,
    pub is_solved: bool,
    pub best_answer_id: Option<Uuid>,
    pub view_count: i32,
    pub reply_count: i32,
    pub last_post_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    // Enriched fields (present on list queries, None on single-thread get)
    pub author: Option<ThreadAuthorResponse>,
    pub category: Option<ThreadCategoryResponse>,
    pub excerpt: Option<String>,
    pub thumbnail_url: Option<String>,
}

impl From<Thread> for ThreadResponse {
    fn from(t: Thread) -> Self {
        let author = t
            .author_username
            .as_ref()
            .map(|username| ThreadAuthorResponse {
                username: username.clone(),
                display_name: t.author_display_name.clone(),
                avatar_url: t.author_avatar_url.clone(),
            });

        let category = t.category_name.as_ref().map(|name| ThreadCategoryResponse {
            slug: t.category_slug.clone(),
            name: name.clone(),
        });

        Self {
            id: t.id,
            category_id: t.category_id,
            category_slug: t.category_slug,
            author_id: t.author_id,
            title: t.title,
            slug: t.slug,
            status: format!("{:?}", t.status).to_lowercase(),
            is_pinned: t.is_pinned,
            is_solved: t.is_solved,
            best_answer_id: t.best_answer_id,
            view_count: t.view_count,
            reply_count: t.reply_count,
            last_post_at: t.last_post_at,
            created_at: t.created_at,
            author,
            category,
            excerpt: t.excerpt,
            thumbnail_url: t.thumbnail_url,
        }
    }
}
