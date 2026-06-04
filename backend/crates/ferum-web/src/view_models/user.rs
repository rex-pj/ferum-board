use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use ferum_domain::models::user::User;

#[derive(Serialize)]
pub struct UserSummaryResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub primary_role_slug: Option<String>,
    pub trust_level: String,
    pub trust_score: i32,
    pub avatar_url: Option<String>,
    pub cover_url: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub post_count: i32,
    pub is_banned: bool,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserSummaryResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
            primary_role_slug: u.primary_role_slug,
            trust_level: format!("{:?}", u.trust_level).to_lowercase(),
            trust_score: u.trust_score,
            avatar_url: u.avatar_url,
            cover_url: u.cover_url,
            bio: u.bio,
            website: u.website,
            post_count: u.post_count,
            is_banned: u.is_banned,
            created_at: u.created_at,
        }
    }
}
