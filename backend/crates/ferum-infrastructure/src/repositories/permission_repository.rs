use async_trait::async_trait;
use sea_orm::*;
use uuid::Uuid;

use ferum_application::shared::AppError;
use ferum_domain::models::role::Permission;
use ferum_domain::models::user::TrustLevel as DomainTrustLevel;
use ferum_domain::repositories::permission_repository::PermissionRepository;

use crate::entities::{permissions, role_permissions, users::TrustLevel as EntityTrustLevel};

pub struct PgPermissionRepository {
    db: DatabaseConnection,
}

impl PgPermissionRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_trust_to_domain(t: EntityTrustLevel) -> DomainTrustLevel {
    match t {
        EntityTrustLevel::New => DomainTrustLevel::New,
        EntityTrustLevel::Basic => DomainTrustLevel::Basic,
        EntityTrustLevel::Member => DomainTrustLevel::Member,
        EntityTrustLevel::Regular => DomainTrustLevel::Regular,
        EntityTrustLevel::Leader => DomainTrustLevel::Leader,
    }
}

fn to_domain(m: permissions::Model) -> Permission {
    Permission {
        id: m.id,
        key: m.key,
        description: m.description,
        group_name: m.group_name,
        min_trust: entity_trust_to_domain(m.min_trust),
    }
}

#[async_trait]
impl PermissionRepository for PgPermissionRepository {
    async fn list_all(&self) -> Result<Vec<Permission>, AppError> {
        let rows = permissions::Entity::find()
            .order_by_asc(permissions::Column::GroupName)
            .order_by_asc(permissions::Column::Key)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn list_for_role(&self, role_id: Uuid) -> Result<Vec<Permission>, AppError> {
        let rows = permissions::Entity::find()
            .inner_join(role_permissions::Entity)
            .filter(role_permissions::Column::RoleId.eq(role_id))
            .order_by_asc(permissions::Column::Key)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn set_role_permissions(
        &self,
        role_id: Uuid,
        permission_keys: &[String],
    ) -> Result<(), AppError> {
        let perms = permissions::Entity::find()
            .filter(permissions::Column::Key.is_in(permission_keys.to_vec()))
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        let perm_ids: Vec<Uuid> = perms.iter().map(|p| p.id).collect();

        role_permissions::Entity::delete_many()
            .filter(role_permissions::Column::RoleId.eq(role_id))
            .exec(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        if perm_ids.is_empty() {
            return Ok(());
        }

        let models: Vec<role_permissions::ActiveModel> = perm_ids
            .into_iter()
            .map(|permission_id| role_permissions::ActiveModel {
                role_id: Set(role_id),
                permission_id: Set(permission_id),
            })
            .collect();

        role_permissions::Entity::insert_many(models)
            .exec(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        Ok(())
    }
}
