use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};

// ─── Requests ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct CreateCategoryRequest {
    #[validate(length(min = 1, max = 80))]
    pub name: String,
    #[validate(length(min = 1, max = 80))]
    pub slug: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub position: i32,
    #[serde(default = "default_view_policy")]
    pub view_policy: String,
    #[serde(default = "default_post_policy")]
    pub post_policy: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCategoryRequest {
    #[validate(length(min = 1, max = 80))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 80))]
    pub slug: Option<String>,
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

pub fn parse_post_policy(s: &str) -> Option<PostPolicy> {
    match s {
        "members" => Some(PostPolicy::Members),
        "trusted" => Some(PostPolicy::Trusted),
        "staff_only" => Some(PostPolicy::StaffOnly),
        "closed" => Some(PostPolicy::Closed),
        _ => None,
    }
}

// ─── Responses ────────────────────────────────────────────────────────────────

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
            }
            .to_string(),
            color: c.color,
            created_at: c.created_at,
        }
    }
}

// ─── Forum index responses ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SubcategoryIndexResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub thread_count: u64,
}

#[derive(Serialize)]
pub struct ForumIndexGroupResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub position: i32,
    pub thread_count: u64,
    pub subcategories: Vec<SubcategoryIndexResponse>,
    pub recent_threads: Vec<crate::view_models::thread::ThreadResponse>,
}
