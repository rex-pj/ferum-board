use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_domain::models::tag::Tag;

#[derive(Serialize)]
pub struct TagResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub color: Option<String>,
}

impl From<Tag> for TagResponse {
    fn from(t: Tag) -> Self {
        Self {
            id: t.id,
            name: t.name,
            slug: t.slug,
            color: t.color,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TagListQuery {
    pub q: Option<String>,
}
