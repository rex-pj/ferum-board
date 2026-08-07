use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::user::TrustLevel;

/// Extracted from JWT + resolved role/permissions by the auth middleware.
/// Carried in request extensions as `Option<AuthUser>`.
/// None = unauthenticated guest.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub trust_level: TrustLevel,
    pub is_banned: bool,
    pub banned_until: Option<DateTime<Utc>>,
    /// Permissions from global (non-category-scoped) role assignments.
    pub permissions: HashSet<String>,
    /// Permissions scoped to a specific category.
    /// key = category_id, value = set of permission keys granted in that category.
    pub category_permissions: HashMap<Uuid, HashSet<String>>,
}

impl AuthUser {
    pub fn is_currently_banned(&self) -> bool {
        if !self.is_banned {
            return false;
        }
        self.banned_until.map(|t| t > Utc::now()).unwrap_or(true)
    }

    /// Returns true if the user has a global (non-scoped) permission.
    pub fn has_perm(&self, key: &str) -> bool {
        self.permissions.contains(key)
    }

    /// Returns true if the user has the permission globally OR scoped to the given category.
    pub fn has_perm_in(&self, key: &str, category_id: Uuid) -> bool {
        self.has_perm(key)
            || self
                .category_permissions
                .get(&category_id)
                .is_some_and(|p| p.contains(key))
    }

    /// Returns true if the user has the permission globally OR in ANY category scope.
    /// Use this when the action is not tied to a specific category (e.g., listing the
    /// report queue without a pre-known category filter).
    pub fn has_perm_any_category(&self, key: &str) -> bool {
        self.has_perm(key)
            || self.category_permissions.values().any(|p| p.contains(key))
    }

    /// Returns `None` if the user holds `key` globally (no filtering needed — they
    /// may see/act on every category), or `Some(ids)` listing exactly the categories
    /// where they hold `key` via a category-scoped role. Used to scope list queries
    /// (e.g. the report queue) so a category-scoped moderator cannot see or act on
    /// content outside their assigned categories.
    pub fn permitted_category_ids(&self, key: &str) -> Option<Vec<Uuid>> {
        if self.has_perm(key) {
            return None;
        }
        Some(
            self.category_permissions
                .iter()
                .filter(|(_, perms)| perms.contains(key))
                .map(|(cat_id, _)| *cat_id)
                .collect(),
        )
    }

    pub fn meets_trust(&self, required: TrustLevel) -> bool {
        self.trust_level >= required
    }
}
