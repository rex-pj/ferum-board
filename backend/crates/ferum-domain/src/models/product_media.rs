use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductMedia {
    pub id: Uuid,
    pub product_id: Uuid,
    /// CAS key into the stored_files blob store.
    pub storage_key: String,
    /// One of: photo | before | after | detail.
    pub kind: String,
    pub caption: Option<String>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

pub struct NewProductMedia {
    pub id: Uuid,
    pub product_id: Uuid,
    pub storage_key: String,
    pub kind: String,
    pub caption: Option<String>,
    pub position: i32,
}
