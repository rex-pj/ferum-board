use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};


#[derive(Debug, Deserialize, Validate)]
pub struct CreateCategoryRequest {
    #[validate(length(min = 1, max = 80))]
    pub name: String,
    #[validate(
        length(min = 1, max = 80),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: String,
    #[validate(length(max = 500, message = "Description must be at most 500 characters"))]
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub position: i32,
    #[serde(default = "default_view_policy")]
    pub view_policy: String,
    #[serde(default = "default_post_policy")]
    pub post_policy: String,
    #[validate(custom(function = "crate::view_models::validators::hex_color"))]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCategoryRequest {
    #[validate(length(min = 1, max = 80))]
    pub name: Option<String>,
    #[validate(
        length(min = 1, max = 80),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: Option<String>,
    // Option<Option<String>> — validated manually in handler (validator derive doesn't handle double-Option)
    pub description: Option<Option<String>>,
    pub parent_id: Option<Option<Uuid>>,
    pub position: Option<i32>,
    pub view_policy: Option<String>,
    pub post_policy: Option<String>,
    pub color: Option<Option<String>>,
}

fn default_view_policy() -> String {
    "public".to_string()
}

fn default_post_policy() -> String {
    "members".to_string()
}

pub fn parse_view_policy(s: &str) -> Option<ViewPolicy> {
    match s {
        "public" => Some(ViewPolicy::Public),
        "members_only" => Some(ViewPolicy::MembersOnly),
        "staff_only" => Some(ViewPolicy::StaffOnly),
        _ => None,
    }
}

/// Accepts every value `CategoryResponse` can emit, `moderated` included —
/// otherwise a client that GETs a moderated category and PATCHes it back is
/// rejected with the API's own output.
pub fn parse_post_policy(s: &str) -> Option<PostPolicy> {
    match s {
        "members" => Some(PostPolicy::Members),
        "trusted" => Some(PostPolicy::Trusted),
        "staff_only" => Some(PostPolicy::StaffOnly),
        "closed" => Some(PostPolicy::Closed),
        "moderated" => Some(PostPolicy::Moderated),
        _ => None,
    }
}


#[derive(Serialize)]
pub struct CategoryResponse {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub position: i32,
    pub view_policy: String,
    pub post_policy: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<Category> for CategoryResponse {
    fn from(c: Category) -> Self {
        Self {
            id: c.id,
            parent_id: c.parent_id,
            slug: c.slug,
            name: c.name,
            description: c.description,
            position: c.position,
            view_policy: match c.view_policy {
                ViewPolicy::Public => "public",
                ViewPolicy::MembersOnly => "members_only",
                ViewPolicy::StaffOnly => "staff_only",
            }
            .to_string(),
            post_policy: match c.post_policy {
                PostPolicy::Members => "members",
                PostPolicy::Trusted => "trusted",
                PostPolicy::StaffOnly => "staff_only",
                PostPolicy::Closed => "closed",
                PostPolicy::Moderated => "moderated",
            }
            .to_string(),
            color: c.color,
            created_at: c.created_at,
        }
    }
}

