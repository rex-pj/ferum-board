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
    //
    // NOT here, and deliberately: `thread.edit_own` and `thread.delete_own`.
    // Editing or deleting your own thread is gated by authorship — the edit
    // window for the former, `require_author_or_mod` for the latter — and never
    // by an RBAC grant. They existed as keys for a long time without a single
    // call site, so revoking them in /admin/permissions did nothing at all. A
    // permission that cannot be enforced is worse than no permission, because
    // the panel promises a control that is not there.
    pub const THREAD_CREATE: &str = "thread.create";
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
    // `link.embed` was removed for the same reason: no markdown, sanitiser or
    // post path has ever consulted it.
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
    /// Manage installed languages: enable/disable, set the site default, and
    /// (later) upload language packs and override individual strings.
    ///
    /// Its own key rather than reusing `admin.config`, so a translator can be
    /// given language access without also handing them SMTP settings and
    /// registration controls.
    pub const ADMIN_LANGUAGES: &str = "admin.languages";
}

// ── System role / permission definitions ──────────────────────────────────────
//
// The catalogue below is the single source of truth for what a permission *is*.
// It used to live as SQL string literals inside four separate migrations, which
// meant a key the code checked against (`perm::` above) and the row the checker
// resolved it from were two independent spellings that nothing kept in step.
// The seeder (`ferum-infrastructure/src/system_seed_service.rs`) reads these
// constants, so adding a permission is a const here plus an entry below — no
// migration.
//
// `min_trust` values are the ones the schema has always carried; they are the
// trust gate, not the grant. Which roles receive a permission by default is
// `SystemRoleDef::grants`, applied only when the permission row is first
// created — see the seeder for why re-granting on every boot would undo an
// admin's deliberate revocation.

/// One permission row, as the system defines it.
pub struct PermissionDef {
    pub key: &'static str,
    pub description: &'static str,
    pub group_name: &'static str,
    pub min_trust: TrustLevel,
}

/// Which permissions a system role holds on a fresh install.
pub enum RoleGrants {
    /// Every permission that exists, including ones added in later versions.
    All,
    List(&'static [&'static str]),
}

/// A non-deletable role the system guarantees exists.
pub struct SystemRoleDef {
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub color: &'static str,
    /// Assigned automatically to new registrations.
    pub is_default: bool,
    pub position: i32,
    pub grants: RoleGrants,
}

pub const PERMISSIONS: &[PermissionDef] = &[
    // ── Content ──────────────────────────────────────────────────────────────
    PermissionDef { key: perm::THREAD_CREATE,     description: "Create new threads",                  group_name: "content", min_trust: TrustLevel::Basic },
    PermissionDef { key: perm::THREAD_EDIT_ANY,   description: "Edit any thread title",               group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::THREAD_DELETE_ANY, description: "Delete any thread",                   group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::THREAD_PIN,        description: "Pin / unpin threads",                 group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::THREAD_LOCK,       description: "Lock / unlock threads",               group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::THREAD_MOVE,       description: "Move threads to another category",    group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::POST_CREATE,       description: "Post replies",                        group_name: "content", min_trust: TrustLevel::Basic },
    PermissionDef { key: perm::POST_EDIT_OWN,     description: "Edit own posts (within 24h)",         group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::POST_DELETE_OWN,   description: "Delete own posts",                    group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::POST_DELETE_ANY,   description: "Delete any post",                     group_name: "content", min_trust: TrustLevel::New },
    PermissionDef { key: perm::REACTION_ADD,      description: "Add reactions to posts",              group_name: "content", min_trust: TrustLevel::Basic },
    PermissionDef { key: perm::FILE_UPLOAD,       description: "Upload files and images",             group_name: "content", min_trust: TrustLevel::Member },
    PermissionDef { key: perm::TAG_CREATE,        description: "Create new tags",                     group_name: "content", min_trust: TrustLevel::Member },
    // ── Catalog ──────────────────────────────────────────────────────────────
    PermissionDef { key: perm::PRODUCT_MANAGE,    description: "Manage the product / material catalog",       group_name: "catalog", min_trust: TrustLevel::New },
    // Crowd-sourced contribution: any member may PROPOSE a product (it lands as
    // `draft`); only `product.manage` can publish it. Basic = email verified,
    // which keeps throwaway bots out.
    PermissionDef { key: perm::PRODUCT_SUBMIT,    description: "Submit a product to the catalog for review",  group_name: "catalog", min_trust: TrustLevel::Basic },
    // ── Moderation ───────────────────────────────────────────────────────────
    PermissionDef { key: perm::REPORT_CREATE,     description: "Report posts / threads",              group_name: "moderation", min_trust: TrustLevel::New },
    PermissionDef { key: perm::MOD_VIEW_REPORTS,  description: "View report queue",                   group_name: "moderation", min_trust: TrustLevel::New },
    PermissionDef { key: perm::MOD_RESOLVE,       description: "Resolve or dismiss reports",          group_name: "moderation", min_trust: TrustLevel::New },
    PermissionDef { key: perm::MOD_WARN,          description: "Warn users",                          group_name: "moderation", min_trust: TrustLevel::New },
    PermissionDef { key: perm::MOD_BAN_TEMP,      description: "Temporarily ban users",               group_name: "moderation", min_trust: TrustLevel::New },
    // ── Admin ────────────────────────────────────────────────────────────────
    PermissionDef { key: perm::ADMIN_USERS,         description: "Manage users and role assignments", group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_BAN_PERMANENT, description: "Permanently ban users",             group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_CATEGORIES,    description: "Create / edit / delete categories", group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_ROLES,         description: "Manage roles and permissions",      group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_CONFIG,        description: "Manage site configuration",         group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_WEBHOOKS,      description: "Manage webhooks",                   group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_PLUGINS,       description: "Install and manage plugins",        group_name: "admin", min_trust: TrustLevel::New },
    PermissionDef { key: perm::ADMIN_LANGUAGES,     description: "Manage site languages and translations", group_name: "admin", min_trust: TrustLevel::New },
];

const MODERATOR_GRANTS: &[&str] = &[
    perm::THREAD_CREATE,
    perm::THREAD_EDIT_ANY, perm::THREAD_DELETE_ANY,
    perm::THREAD_PIN, perm::THREAD_LOCK, perm::THREAD_MOVE,
    perm::POST_CREATE, perm::POST_EDIT_OWN, perm::POST_DELETE_OWN, perm::POST_DELETE_ANY,
    perm::REACTION_ADD, perm::FILE_UPLOAD, perm::TAG_CREATE,
    perm::PRODUCT_SUBMIT,
    perm::REPORT_CREATE,
    perm::MOD_VIEW_REPORTS, perm::MOD_RESOLVE,
    perm::MOD_WARN, perm::MOD_BAN_TEMP,
];

const MEMBER_GRANTS: &[&str] = &[
    perm::THREAD_CREATE,
    perm::POST_CREATE, perm::POST_EDIT_OWN, perm::POST_DELETE_OWN,
    perm::REACTION_ADD, perm::FILE_UPLOAD, perm::TAG_CREATE,
    perm::PRODUCT_SUBMIT,
    perm::REPORT_CREATE,
];

pub const SYSTEM_ROLES: &[SystemRoleDef] = &[
    SystemRoleDef {
        slug: "admin",
        name: "Administrator",
        description: "Full system access",
        color: "#dc3545",
        is_default: false,
        position: 0,
        grants: RoleGrants::All,
    },
    SystemRoleDef {
        slug: "moderator",
        name: "Moderator",
        description: "Category-scoped moderation permissions",
        color: "#fd7e14",
        is_default: false,
        position: 1,
        grants: RoleGrants::List(MODERATOR_GRANTS),
    },
    SystemRoleDef {
        slug: "member",
        name: "Member",
        description: "Default member role",
        color: "#0d6efd",
        is_default: true,
        position: 2,
        grants: RoleGrants::List(MEMBER_GRANTS),
    },
];

impl SystemRoleDef {
    pub fn grants_key(&self, key: &str) -> bool {
        match self.grants {
            RoleGrants::All => true,
            RoleGrants::List(keys) => keys.contains(&key),
        }
    }
}
