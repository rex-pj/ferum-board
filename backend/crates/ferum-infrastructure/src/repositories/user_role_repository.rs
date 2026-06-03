use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::*;
use uuid::Uuid;

use ferum_application::shared::AppError;
use ferum_domain::models::role::UserRoleAssignment;
use ferum_domain::repositories::user_role_repository::UserRoleRepository;

use crate::entities::user_roles;

pub struct PgUserRoleRepository {
    db: DatabaseConnection,
}

impl PgUserRoleRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[derive(FromQueryResult)]
struct UserRoleRow {
    id: Uuid,
    user_id: Uuid,
    role_id: Uuid,
    role_slug: String,
    category_id: Option<Uuid>,
    granted_by: Option<Uuid>,
    expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    created_at: chrono::DateTime<chrono::FixedOffset>,
}

fn to_domain(row: UserRoleRow) -> UserRoleAssignment {
    UserRoleAssignment {
        id: row.id,
        user_id: row.user_id,
        role_id: row.role_id,
        role_slug: row.role_slug,
        category_id: row.category_id,
        granted_by: row.granted_by,
        expires_at: row.expires_at.map(|t| t.with_timezone(&Utc)),
        created_at: row.created_at.with_timezone(&Utc),
    }
}

const SELECT: &str = r#"
    SELECT
        ur.id, ur.user_id, ur.role_id,
        r.slug AS role_slug,
        ur.category_id, ur.granted_by, ur.expires_at, ur.created_at
    FROM user_roles ur
    JOIN roles r ON r.id = ur.role_id
"#;

#[async_trait]
impl UserRoleRepository for PgUserRoleRepository {
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<UserRoleAssignment>, AppError> {
        let rows = UserRoleRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            &format!("{SELECT} WHERE ur.user_id = $1 AND (ur.expires_at IS NULL OR ur.expires_at > now())"),
            [user_id.into()],
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn list_for_category(
        &self,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<Vec<UserRoleAssignment>, AppError> {
        let rows = match category_id {
            Some(cat_id) => UserRoleRow::find_by_statement(Statement::from_sql_and_values(
                DbBackend::Postgres,
                &format!("{SELECT} WHERE ur.role_id = $1 AND ur.category_id = $2"),
                [role_id.into(), cat_id.into()],
            )),
            None => UserRoleRow::find_by_statement(Statement::from_sql_and_values(
                DbBackend::Postgres,
                &format!("{SELECT} WHERE ur.role_id = $1 AND ur.category_id IS NULL"),
                [role_id.into()],
            )),
        }
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn assign(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
        granted_by: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<UserRoleAssignment, AppError> {
        let model = user_roles::ActiveModel {
            user_id: Set(user_id),
            role_id: Set(role_id),
            category_id: Set(category_id),
            granted_by: Set(Some(granted_by)),
            expires_at: Set(expires_at.map(|t| t.into())),
            ..Default::default()
        };

        let row = user_roles::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|e| {
                if e.to_string().contains("duplicate") || e.to_string().contains("unique") {
                    AppError::Conflict("role_already_assigned".to_string())
                } else {
                    AppError::internal(e.to_string())
                }
            })?;

        // Fetch the full row with role slug joined
        let rows = UserRoleRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            &format!("{SELECT} WHERE ur.id = $1"),
            [row.id.into()],
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

        rows.into_iter()
            .next()
            .map(to_domain)
            .ok_or(AppError::NotFound)
    }

    async fn revoke(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let mut q = user_roles::Entity::delete_many()
            .filter(user_roles::Column::UserId.eq(user_id))
            .filter(user_roles::Column::RoleId.eq(role_id));

        q = match category_id {
            Some(cat_id) => q.filter(user_roles::Column::CategoryId.eq(cat_id)),
            None => q.filter(user_roles::Column::CategoryId.is_null()),
        };

        let result = q
            .exec(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        if result.rows_affected == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    async fn is_assigned(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        category_id: Option<Uuid>,
    ) -> Result<bool, AppError> {
        let mut q = user_roles::Entity::find()
            .filter(user_roles::Column::UserId.eq(user_id))
            .filter(user_roles::Column::RoleId.eq(role_id));

        q = match category_id {
            Some(cat_id) => q.filter(user_roles::Column::CategoryId.eq(cat_id)),
            None => q.filter(user_roles::Column::CategoryId.is_null()),
        };

        let count = q
            .count(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(count > 0)
    }
}
