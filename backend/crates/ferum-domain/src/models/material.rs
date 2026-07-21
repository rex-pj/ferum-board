use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    /// One of: wood_natural | wood_engineered | rattan_bamboo | metal | fabric
    /// | leather | stone | glass | plastic | other.
    pub category: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct NewMaterial {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub category: String,
    pub description: Option<String>,
}

/// Partial update. `None` = leave unchanged. Slug is immutable (stable key).
#[derive(Debug, Default, Clone)]
pub struct UpdateMaterial {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<Option<String>>,
}
