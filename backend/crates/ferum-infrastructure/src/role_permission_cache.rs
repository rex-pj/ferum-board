use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::sea_query::{Alias, Expr, PostgresQueryBuilder, Query};
use sea_orm::*;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::entities::{permissions, role_permissions};
use ferum_application::ports::PermissionResolver;
use ferum_application::shared::AppError;
use ferum_domain::models::role::UserRoleAssignment;

/// Resolves role_id → Set<permission_key> entirely in-memory.
/// Reloaded from DB on startup and invalidated on admin role-permission changes.
#[derive(Clone)]
pub struct RolePermissionCache {
    db: DatabaseConnection,
    /// role_id → set of permission keys
    inner: Arc<RwLock<HashMap<Uuid, HashSet<String>>>>,
    /// permission key → min_trust string (loaded alongside role_permissions)
    min_trust: Arc<RwLock<HashMap<String, String>>>,
}

impl RolePermissionCache {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            inner: Arc::new(RwLock::new(HashMap::new())),
            min_trust: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load / reload the full role→permission mapping from DB.
    /// Also loads permission key → min_trust so `resolve_min_trust` never hits the DB.
    pub async fn load(&self) -> Result<(), AppError> {
        #[derive(FromQueryResult)]
        struct Row {
            role_id: Option<Uuid>,
            perm_key: String,
            min_trust: String,
        }

        // Single query: all permissions + their role assignments (if any).
        // LEFT JOIN ensures permissions with no role assignment still appear
        // so min_trust is fully populated regardless of RBAC state.
        let (sql, values) = Query::select()
            .column((role_permissions::Entity, role_permissions::Column::RoleId))
            .expr_as(
                Expr::col((permissions::Entity, permissions::Column::Key)),
                Alias::new("perm_key"),
            )
            .expr_as(
                Expr::col((permissions::Entity, permissions::Column::MinTrust))
                    .cast_as(Alias::new("text")),
                Alias::new("min_trust"),
            )
            .from(permissions::Entity)
            .left_join(
                role_permissions::Entity,
                Expr::col((role_permissions::Entity, role_permissions::Column::PermissionId))
                    .equals((permissions::Entity, permissions::Column::Id)),
            )
            .build(PostgresQueryBuilder);

        let rows = Row::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

        let mut perm_map: HashMap<Uuid, HashSet<String>> = HashMap::new();
        let mut trust_map: HashMap<String, String> = HashMap::new();
        for row in rows {
            trust_map.entry(row.perm_key.clone()).or_insert(row.min_trust);
            if let Some(role_id) = row.role_id {
                perm_map.entry(role_id).or_default().insert(row.perm_key);
            }
        }

        *self.inner.write().await = perm_map;
        *self.min_trust.write().await = trust_map;
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
    /// Served entirely from the in-memory map populated by `load()` — no DB query.
    pub async fn resolve_min_trust(&self) -> Result<HashMap<String, String>, AppError> {
        Ok(self.min_trust.read().await.clone())
    }
}

#[async_trait]
impl PermissionResolver for RolePermissionCache {
    async fn resolve_global(&self, assignments: &[UserRoleAssignment]) -> HashSet<String> {
        self.resolve_global(assignments).await
    }

    async fn resolve_category(
        &self,
        assignments: &[UserRoleAssignment],
    ) -> HashMap<Uuid, HashSet<String>> {
        self.resolve_category(assignments).await
    }

    async fn permissions_for_role(&self, role_id: Uuid) -> Vec<String> {
        self.permissions_for_role(role_id).await
    }

    async fn reload(&self) -> Result<(), AppError> {
        self.load().await
    }
}
