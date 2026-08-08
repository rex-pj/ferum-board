use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use uuid::Uuid;

use ferum_application::ports::PermissionResolver;
use ferum_application::shared::AppError;
use ferum_domain::models::role::UserRoleAssignment;

mockall::mock! {
    pub PermissionResolver {}

    #[async_trait]
    impl PermissionResolver for PermissionResolver {
        async fn resolve_global(&self, assignments: &[UserRoleAssignment]) -> HashSet<String>;
        async fn resolve_category(&self, assignments: &[UserRoleAssignment]) -> HashMap<Uuid, HashSet<String>>;
        async fn permissions_for_role(&self, role_id: Uuid) -> Vec<String>;
        async fn reload(&self) -> Result<(), AppError>;
    }
}

/// Resolves every assignment set to one fixed global permission set.
///
/// Most callers only care whether the *target* of a moderation action comes back
/// as staff or not, and building a role graph to express that is noise. Pass the
/// permission keys the target should be treated as holding.
pub struct FixedPermissionResolver {
    pub global: HashSet<String>,
    pub category: HashMap<Uuid, HashSet<String>>,
}

impl FixedPermissionResolver {
    /// A user with no permissions at all — an ordinary member.
    pub fn none() -> Self {
        Self { global: HashSet::new(), category: HashMap::new() }
    }

    pub fn global(perms: &[&str]) -> Self {
        Self {
            global: perms.iter().map(|p| p.to_string()).collect(),
            category: HashMap::new(),
        }
    }

    /// Permissions held only inside one category — a category-scoped moderator,
    /// who is still staff when viewed from anywhere else.
    pub fn in_category(category_id: Uuid, perms: &[&str]) -> Self {
        Self {
            global: HashSet::new(),
            category: HashMap::from([(
                category_id,
                perms.iter().map(|p| p.to_string()).collect(),
            )]),
        }
    }
}

#[async_trait]
impl PermissionResolver for FixedPermissionResolver {
    async fn resolve_global(&self, _assignments: &[UserRoleAssignment]) -> HashSet<String> {
        self.global.clone()
    }
    async fn resolve_category(
        &self,
        _assignments: &[UserRoleAssignment],
    ) -> HashMap<Uuid, HashSet<String>> {
        self.category.clone()
    }
    async fn permissions_for_role(&self, _role_id: Uuid) -> Vec<String> {
        self.global.iter().cloned().collect()
    }
    async fn reload(&self) -> Result<(), AppError> {
        Ok(())
    }
}
