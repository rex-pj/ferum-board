#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub is_email_verified: bool,
    pub display_name: Option<String>,
    pub password_hash: Option<String>,
    pub trust_level: TrustLevel,
    /// Slug of the user's highest-priority global role (lowest position value). Used for badge display.
    pub primary_role_slug: Option<String>,
    pub trust_score: i32,
    pub post_count: i32,
    pub days_visited: i32,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub is_banned: bool,
    pub banned_until: Option<DateTime<Utc>>,
    pub ban_reason: Option<String>,
    pub warn_count: i32,
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

impl User {
    pub fn is_active(&self) -> bool {
        self.deleted_at.is_none()
    }

    pub fn display(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }

    pub fn is_account_locked(&self) -> bool {
        self.locked_until.map(|t| t > Utc::now()).unwrap_or(false)
    }

    pub fn is_currently_banned(&self) -> bool {
        if !self.is_banned {
            return false;
        }
        self.banned_until.map(|t| t > Utc::now()).unwrap_or(true)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash, Copy)]
pub enum TrustLevel {
    New = 0,
    Basic = 1,
    Member = 2,
    Regular = 3,
    Leader = 4,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserPreferences {
    pub user_id: Uuid,
    pub theme: String,
    pub font_size: String,
    pub layout: String,
    pub email_notifications: serde_json::Value,
    pub muted_categories: Vec<Uuid>,
    pub watched_categories: Vec<Uuid>,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            user_id: Uuid::nil(),
            theme: "auto".to_string(),
            font_size: "medium".to_string(),
            layout: "comfortable".to_string(),
            email_notifications: serde_json::json!({}),
            muted_categories: vec![],
            watched_categories: vec![],
        }
    }
}
