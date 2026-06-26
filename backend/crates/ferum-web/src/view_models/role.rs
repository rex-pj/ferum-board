use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::role::{Permission, Role};

// ─── Responses ────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct RoleResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub is_system: bool,
    pub is_default: bool,
    pub position: i32,
}

impl From<Role> for RoleResponse {
    fn from(r: Role) -> Self {
        Self {
            id: r.id,
            slug: r.slug,
            name: r.name,
            description: r.description,
            color: r.color,
            is_system: r.is_system,
            is_default: r.is_default,
            position: r.position,
        }
    }
}

#[derive(Serialize)]
pub struct PermissionResponse {
    pub id: Uuid,
    pub key: String,
    pub description: String,
    pub group_name: String,
    pub min_trust: String,
}

impl From<Permission> for PermissionResponse {
    fn from(p: Permission) -> Self {
        Self {
            id: p.id,
            key: p.key,
            description: p.description,
            group_name: p.group_name,
            min_trust: format!("{:?}", p.min_trust).to_lowercase(),
        }
    }
}

#[derive(Serialize)]
pub struct UserRoleResponse {
    pub id: Uuid,
    pub role: RoleResponse,
    pub category_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Flat list of permission keys granted by this role assignment.
    /// Used by the frontend to gate access without relying on role slug matching.
    pub permissions: Vec<String>,
}

// ─── Requests ─────────────────────────────────────────────────────────────────


#[derive(Debug, Deserialize, Validate)]
pub struct CreateRoleRequest {
    #[validate(
        length(min = 2, max = 50),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: String,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(max = 500, message = "Description must be at most 500 characters"))]
    pub description: Option<String>,
    #[validate(custom(function = "crate::view_models::validators::hex_color"))]
    pub color: Option<String>,
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateRoleRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    // Option<Option<String>> — validated manually in handler (validator derive doesn't handle double-Option)
    pub description: Option<Option<String>>,
    pub color: Option<Option<String>>,
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SetPermissionsRequest {
    pub permission_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub role_id: Uuid,
    pub category_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}
