use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::user::TrustLevel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub is_system: bool,
    pub is_default: bool,
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permission {
    pub id: Uuid,
    pub key: String,
    pub description: String,
    pub group_name: String,
    pub min_trust: TrustLevel,
}

/// A user's role assignment — optionally scoped to a category.
/// `category_id == None` means the role applies globally.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRoleAssignment {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub role_slug: String,
    pub role_name: String,
    pub role_color: Option<String>,
    pub category_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl UserRoleAssignment {
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|t| t <= Utc::now()).unwrap_or(false)
    }
}

// ── Permission key constants ──────────────────────────────────────────────────
pub mod perm {
    // Content
    pub const THREAD_CREATE: &str = "thread.create";
    pub const THREAD_EDIT_OWN: &str = "thread.edit_own";
    pub const THREAD_DELETE_OWN: &str = "thread.delete_own";
    pub const THREAD_EDIT_ANY: &str = "thread.edit_any";
    pub const THREAD_DELETE_ANY: &str = "thread.delete_any";
    pub const THREAD_PIN: &str = "thread.pin";
    pub const THREAD_LOCK: &str = "thread.lock";
    pub const THREAD_MOVE: &str = "thread.move";

    pub const POST_CREATE: &str = "post.create";
    pub const POST_EDIT_OWN: &str = "post.edit_own";
    pub const POST_DELETE_OWN: &str = "post.delete_own";
    pub const POST_DELETE_ANY: &str = "post.delete_any";

    pub const REACTION_ADD: &str = "reaction.add";
    pub const FILE_UPLOAD: &str = "file.upload";
    pub const LINK_EMBED: &str = "link.embed";
    pub const TAG_CREATE: &str = "tag.create";

    // Catalog (furniture review) — curation of the product/material catalog.
    pub const PRODUCT_MANAGE: &str = "product.manage";
    // Crowd-sourced: propose a product (lands as draft, admin publishes).
    pub const PRODUCT_SUBMIT: &str = "product.submit";

    // Moderation
    pub const REPORT_CREATE: &str = "report.create";
    pub const MOD_VIEW_REPORTS: &str = "moderation.view_reports";
    pub const MOD_RESOLVE: &str = "moderation.resolve";
    pub const MOD_WARN: &str = "moderation.warn";
    pub const MOD_BAN_TEMP: &str = "moderation.ban_temp";

    // Admin
    pub const ADMIN_USERS: &str = "admin.users";
    pub const ADMIN_BAN_PERMANENT: &str = "admin.ban_permanent";
    pub const ADMIN_CATEGORIES: &str = "admin.categories";
    pub const ADMIN_ROLES: &str = "admin.roles";
    pub const ADMIN_CONFIG: &str = "admin.config";
    pub const ADMIN_WEBHOOKS: &str = "admin.webhooks";
    pub const ADMIN_PLUGINS: &str = "admin.plugins";
}
