use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A catalogue category — Sofa, Ghế, Bàn, Tủ & Kệ …
///
/// Deliberately not the forum's [`Category`](crate::models::category::Category).
/// That tree organises *discussions* (General Discussion, Off-Topic,
/// Technology); this one organises *furniture*. Sharing one tree between them
/// was the mistake this type exists to undo: no forum category is a sensible
/// home for a sofa, so the column could never be populated and the filter built
/// on it could never return anything.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductCategory {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    /// Two levels, matching the forum tree's shape — `Sofa` → `Sofa góc`.
    pub parent_id: Option<Uuid>,
    pub position: i32,
    /// FontAwesome class, e.g. `fa-couch`. Optional; the UI falls back to a
    /// generic glyph.
    pub icon: Option<String>,
    /// Words that identify this category inside a product name, stored
    /// unaccented and lowercase. Drives auto-assignment: Vietnamese furniture
    /// names lead with the type ("Sofa da Milano", "Ghế ăn Bắc Âu"), so the
    /// name itself is real evidence rather than a guess.
    ///
    /// Data, not code, so an admin can teach the matcher a new word and re-run
    /// it without a deploy.
    pub match_keywords: Vec<String>,
    pub created_at: DateTime<Utc>,
}

pub struct NewProductCategory {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub icon: Option<String>,
    pub match_keywords: Vec<String>,
}

/// Partial update. `None` = leave unchanged. Slug is immutable (stable key).
#[derive(Debug, Default, Clone)]
pub struct UpdateProductCategory {
    pub name: Option<String>,
    pub parent_id: Option<Option<Uuid>>,
    pub position: Option<i32>,
    pub icon: Option<Option<String>>,
    pub match_keywords: Option<Vec<String>>,
}
