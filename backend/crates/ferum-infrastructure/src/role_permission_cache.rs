use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use sea_orm::*;
use tokio::sync::RwLock;
use uuid::Uuid;

use ferum_application::shared::AppError;
use ferum_domain::models::role::UserRoleAssignment;

use crate::entities::permissions;

/// Resolves role_id → Set<permission_key> entirely in-memory.
/// Reloaded from DB on startup and invalidated on admin role-permission changes.
#[derive(Clone)]
pub struct RolePermissionCache {
    db: DatabaseConnection,
    /// role_id → set of permission keys
    inner: Arc<RwLock<HashMap<Uuid, HashSet<String>>>>,
}

impl RolePermissionCache {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load / reload the full role→permission mapping from DB.
    pub async fn load(&self) -> Result<(), AppError> {
        #[derive(FromQueryResult)]
        struct Row {
            role_id: Uuid,
            perm_key: String,
        }

        let rows = Row::find_by_statement(Statement::from_string(
            DbBackend::Postgres,
            r#"
                SELECT rp.role_id, p.key AS perm_key
                FROM role_permissions rp
                JOIN permissions p ON p.id = rp.permission_id
            "#,
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

        let mut map: HashMap<Uuid, HashSet<String>> = HashMap::new();
        for row in rows {
            map.entry(row.role_id).or_default().insert(row.perm_key);
        }

        *self.inner.write().await = map;
        Ok(())
    }

    /// Resolve the effective global permissions for a set of role assignments.
    pub async fn resolve_global(
        &self,
        assignments: &[UserRoleAssignment],
    ) -> HashSet<String> {
        let map = self.inner.read().await;
        let mut result = HashSet::new();
        for a in assignments {
            if a.category_id.is_none() {
                if let Some(perms) = map.get(&a.role_id) {
                    result.extend(perms.iter().cloned());
                }
            }
        }
        result
    }

    /// Resolve per-category permissions: category_id → Set<permission_key>.
    pub async fn resolve_category(
        &self,
        assignments: &[UserRoleAssignment],
    ) -> HashMap<Uuid, HashSet<String>> {
        let map = self.inner.read().await;
        let mut result: HashMap<Uuid, HashSet<String>> = HashMap::new();
        for a in assignments {
            if let Some(cat_id) = a.category_id {
                if let Some(perms) = map.get(&a.role_id) {
                    result.entry(cat_id).or_default().extend(perms.iter().cloned());
                }
            }
        }
        result
    }

    /// Return the permission keys assigned to a single role.
    pub async fn permissions_for_role(&self, role_id: Uuid) -> Vec<String> {
        self.inner
            .read()
            .await
            .get(&role_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Resolve min_trust requirements for all permissions (key → min_trust string).
    pub async fn resolve_min_trust(&self) -> Result<HashMap<String, String>, AppError> {
        let rows = permissions::Entity::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|p| (p.key, p.min_trust.to_value()))
            .collect())
    }
}
