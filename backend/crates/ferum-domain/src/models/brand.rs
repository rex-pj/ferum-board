use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Brand {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub website: Option<String>,
    pub country: Option<String>,
    pub is_verified: bool,
    pub owner_user_id: Option<Uuid>,
    pub tier: String,
    pub created_at: DateTime<Utc>,
}

pub struct NewBrand {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub website: Option<String>,
    pub country: Option<String>,
    pub owner_user_id: Option<Uuid>,
}

/// Partial update. `None` = leave unchanged. Slug is immutable (stable key).
#[derive(Debug, Default, Clone)]
pub struct UpdateBrand {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub website: Option<Option<String>>,
    pub country: Option<Option<String>>,
    pub is_verified: Option<bool>,
}
