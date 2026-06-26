use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub author: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub parent_slug: String,
    pub is_system: bool,
    pub is_active: bool,
    pub preview_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct NewTheme {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub author: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub parent_slug: String,
}
