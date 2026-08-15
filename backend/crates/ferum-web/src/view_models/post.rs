use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::view_models::reaction::ReactionCountResponse;
use crate::view_models::AuthorInfo;
use ferum_domain::models::post::Post;


#[derive(Debug, Deserialize, Validate)]
pub struct CreatePostRequest {
    pub parent_id: Option<Uuid>,
    #[validate(length(min = 1, max = 50_000, message = "Content must be 1–50 000 characters"))]
    pub content_md: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdatePostRequest {
    #[validate(length(min = 1, max = 50_000, message = "Content must be 1–50 000 characters"))]
    pub content_md: String,
}

#[derive(Debug, Deserialize)]
pub struct PostListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}


#[derive(Serialize)]
pub struct PostResponse {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
    pub content_html: String,
    pub status: String,
    pub is_deleted: bool,
    pub edited_at: Option<DateTime<Utc>>,
    pub edit_count: i32,
    pub created_at: DateTime<Utc>,
    pub author: Option<AuthorInfo>,
    pub reactions: Vec<ReactionCountResponse>,
    pub my_reactions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_title: Option<String>,
}

impl From<Post> for PostResponse {
    fn from(p: Post) -> Self {
        let author = p
            .author_username
            .as_ref()
            .map(|username| AuthorInfo {
                username: username.clone(),
                display_name: p.author_display_name.clone(),
                avatar_url: p.author_avatar_url.clone(),
                role: p.author_role.clone(),
            });

        // Redacted for a deleted post. The row is still serialised — clients
        // render a tombstone and dropping it would renumber every later post —
        // but the text must not ride along. See `Post::readable_content`.
        //
        // Read before `reactions` is moved out of `p` below.
        let (content_md, content_html) = {
            let (md, html) = p.readable_content();
            (md.to_string(), html.to_string())
        };

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
            content_md,
            content_html,
            status: format!("{:?}", p.status).to_lowercase(),
            is_deleted: p.is_deleted,
            edited_at: p.edited_at,
            edit_count: p.edit_count,
            created_at: p.created_at,
            author,
            reactions,
            my_reactions,
            thread_slug: p.thread_slug,
            thread_title: p.thread_title,
        }
    }
}
