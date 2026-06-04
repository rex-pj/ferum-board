use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub color: Option<String>,
    pub created_by_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct NewTag {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub color: Option<String>,
    pub created_by_id: Option<Uuid>,
}
