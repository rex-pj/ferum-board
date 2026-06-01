use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::view_models::reaction::ReactionCountResponse;
use ferum_domain::models::post::Post;

// ─── Requests ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct CreatePostRequest {
    pub parent_id: Option<Uuid>,
    #[validate(length(min = 1, message = "Content is required"))]
    pub content_md: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdatePostRequest {
    #[validate(length(min = 1, message = "Content is required"))]
    pub content_md: String,
}

#[derive(Debug, Deserialize)]
pub struct PostListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

// ─── Responses ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PostAuthorResponse {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<String>,
}

#[derive(Serialize)]
pub struct PostResponse {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
    pub content_html: String,
    pub is_deleted: bool,
    pub edited_at: Option<DateTime<Utc>>,
    pub edit_count: i32,
    pub created_at: DateTime<Utc>,
    pub author: Option<PostAuthorResponse>,
    pub reactions: Vec<ReactionCountResponse>,
    pub my_reactions: Vec<String>,
}

impl From<Post> for PostResponse {
    fn from(p: Post) -> Self {
        let author = p
            .author_username
            .as_ref()
            .map(|username| PostAuthorResponse {
                username: username.clone(),
                display_name: p.author_display_name.clone(),
                avatar_url: p.author_avatar_url.clone(),
                role: p.author_role.clone(),
            });

        let reactions = p
            .reactions
            .into_iter()
            .map(|(kind, count)| ReactionCountResponse {
                kind: kind.as_str().to_string(),
                count,
            })
            .collect();

        let my_reactions = p
            .my_reactions
            .iter()
            .map(|k| k.as_str().to_string())
            .collect();

        Self {
            id: p.id,
            thread_id: p.thread_id,
            author_id: p.author_id,
            parent_id: p.parent_id,
            content_md: p.content_md,
            content_html: p.content_html,
            is_deleted: p.is_deleted,
            edited_at: p.edited_at,
            edit_count: p.edit_count,
            created_at: p.created_at,
            author,
            reactions,
            my_reactions,
        }
    }
}
