use async_trait::async_trait;
use sea_orm::*;
use uuid::Uuid;

use ferum_application::shared::AppError;
use ferum_domain::models::role::Role;
use ferum_domain::repositories::role_repository::{NewRole, RoleRepository, UpdateRole};

use crate::entities::roles;

pub struct PgRoleRepository {
    db: DatabaseConnection,
}

impl PgRoleRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: roles::Model) -> Role {
    Role {
        id: m.id,
        slug: m.slug,
        name: m.name,
        description: m.description,
        color: m.color,
        is_system: m.is_system,
        is_default: m.is_default,
        position: m.position,
        created_at: m.created_at.with_timezone(&chrono::Utc),
    }
}

#[async_trait]
impl RoleRepository for PgRoleRepository {
    async fn list(&self) -> Result<Vec<Role>, AppError> {
        let rows = roles::Entity::find()
            .order_by_asc(roles::Column::Position)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Role>, AppError> {
        let row = roles::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(row.map(to_domain))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Role>, AppError> {
        let row = roles::Entity::find()
            .filter(roles::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(row.map(to_domain))
    }

    async fn create(&self, new: NewRole) -> Result<Role, AppError> {
        let model = roles::ActiveModel {
            slug: Set(new.slug),
            name: Set(new.name),
            description: Set(new.description),
            color: Set(new.color),
            is_system: Set(false),
            is_default: Set(false),
            position: Set(new.position),
            ..Default::default()
        };
        let row = roles::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(to_domain(row))
    }

    async fn update(&self, id: Uuid, patch: UpdateRole) -> Result<Role, AppError> {
        let mut model: roles::ActiveModel = roles::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .ok_or(AppError::NotFound)?
            .into();

        if let Some(name) = patch.name {
            model.name = Set(name);
        }
        if let Some(description) = patch.description {
            model.description = Set(description);
        }
        if let Some(color) = patch.color {
            model.color = Set(color);
        }
        if let Some(position) = patch.position {
            model.position = Set(position);
        }

        let row = model
            .update(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(to_domain(row))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let row = roles::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .ok_or(AppError::NotFound)?;

        if row.is_system {
            return Err(AppError::forbidden("cannot_delete_system_role"));
        }

        roles::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(())
    }
}
