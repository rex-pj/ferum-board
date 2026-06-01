use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::ports::SearchHit;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub category_id: Option<Uuid>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

#[derive(Serialize)]
pub struct SearchHitResponse {
    pub thread_id: Uuid,
    pub thread_slug: String,
    pub title: String,
    pub excerpt: Option<String>,
}

impl From<SearchHit> for SearchHitResponse {
    fn from(h: SearchHit) -> Self {
        Self {
            thread_id: h.thread_id,
            thread_slug: h.thread_slug,
            title: h.title,
            excerpt: h.excerpt,
        }
    }
}
