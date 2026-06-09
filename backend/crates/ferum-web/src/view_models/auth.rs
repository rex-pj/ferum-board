use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::user::User;

use super::role::UserRoleResponse;

// ─── Requests ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 30, message = "Username must be 3–30 characters"))]
    pub username: String,
    #[validate(email(message = "Invalid email address"))]
    pub email: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub new_password: String,
}

// ─── Responses ────────────────────────────────────────────────────────────────

/// Returned by GET /api/users/me and auth flows.
/// `roles` is populated separately by the handler after fetching user_roles.
#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    /// Slug of the user's highest-priority global role (for badge display).
    pub primary_role_slug: Option<String>,
    pub trust_level: String,
    pub trust_score: i32,
    pub avatar_url: Option<String>,
    pub cover_url: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub post_count: i32,
    pub is_banned: bool,
    pub ban_reason: Option<String>,
    pub banned_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Populated when the caller needs full role info (e.g. GET /api/users/me).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<UserRoleResponse>>,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            email: u.email,
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
            ban_reason: u.ban_reason,
            banned_until: u.banned_until,
            created_at: u.created_at,
            roles: None, // populated by handler when needed
        }
    }
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub user: UserResponse,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
}
