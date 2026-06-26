use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Alias, Expr, PostgresQueryBuilder, Query, SelectStatement, SimpleExpr};
use sea_orm::*;
use std::collections::HashMap;
use uuid::Uuid;

use ferum_application::shared::AppError;
use ferum_domain::models::role::UserRoleAssignment;
use ferum_domain::repositories::user_role_repository::UserRoleRepository;

use crate::entities::{roles, user_roles};

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
    role_name: String,
    role_color: Option<String>,
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
        role_name: row.role_name,
        role_color: row.role_color,
        category_id: row.category_id,
        granted_by: row.granted_by,
        expires_at: row.expires_at.map(|t| t.with_timezone(&Utc)),
        created_at: row.created_at.with_timezone(&Utc),
    }
}

/// Base SELECT + JOIN shared by all read methods. Each caller appends its own
/// WHERE conditions before calling `.build(PostgresQueryBuilder)`.
fn base_query() -> SelectStatement {
    Query::select()
        .column((user_roles::Entity, user_roles::Column::Id))
        .column((user_roles::Entity, user_roles::Column::UserId))
        .column((user_roles::Entity, user_roles::Column::RoleId))
        .expr_as(
            Expr::col((roles::Entity, roles::Column::Slug)),
            Alias::new("role_slug"),
        )
        .expr_as(
            Expr::col((roles::Entity, roles::Column::Name)),
            Alias::new("role_name"),
        )
        .expr_as(
            Expr::col((roles::Entity, roles::Column::Color)),
            Alias::new("role_color"),
        )
        .column((user_roles::Entity, user_roles::Column::CategoryId))
        .column((user_roles::Entity, user_roles::Column::GrantedBy))
        .column((user_roles::Entity, user_roles::Column::ExpiresAt))
        .column((user_roles::Entity, user_roles::Column::CreatedAt))
        .from(user_roles::Entity)
        .inner_join(
            roles::Entity,
            Expr::col((roles::Entity, roles::Column::Id))
                .equals((user_roles::Entity, user_roles::Column::RoleId)),
        )
        .to_owned()
}

/// Condition: row is not expired (expires_at IS NULL OR expires_at > now()).
fn not_expired() -> SimpleExpr {
    Expr::col((user_roles::Entity, user_roles::Column::ExpiresAt))
        .is_null()
        .or(Expr::col((user_roles::Entity, user_roles::Column::ExpiresAt))
            .gt(Expr::cust("now()")))
}

#[async_trait]
impl UserRoleRepository for PgUserRoleRepository {
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<UserRoleAssignment>, AppError> {
        let (sql, values) = base_query()
            .and_where(
                Expr::col((user_roles::Entity, user_roles::Column::UserId)).eq(user_id),
            )
            .and_where(not_expired())
            .build(PostgresQueryBuilder);

        let rows = UserRoleRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
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
        let mut q = base_query();
        q.and_where(Expr::col((user_roles::Entity, user_roles::Column::RoleId)).eq(role_id));
        match category_id {
            Some(cat_id) => q.and_where(
                Expr::col((user_roles::Entity, user_roles::Column::CategoryId)).eq(cat_id),
            ),
            None => q.and_where(
                Expr::col((user_roles::Entity, user_roles::Column::CategoryId)).is_null(),
            ),
        };
        let (sql, values) = q.build(PostgresQueryBuilder);

        let rows = UserRoleRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
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
        let (sql, values) = base_query()
            .and_where(Expr::col((user_roles::Entity, user_roles::Column::Id)).eq(row.id))
            .build(PostgresQueryBuilder);

        let rows = UserRoleRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
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

    async fn list_for_users(&self, user_ids: &[Uuid]) -> Result<Vec<UserRoleAssignment>, AppError> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders: Vec<SimpleExpr> =
            user_ids.iter().map(|id| Expr::val(*id).into()).collect();

        let (sql, values) = base_query()
            .and_where(
                Expr::col((user_roles::Entity, user_roles::Column::UserId))
                    .is_in(placeholders),
            )
            .and_where(not_expired())
            .build(PostgresQueryBuilder);

        let rows = UserRoleRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn count_by_role(&self) -> Result<HashMap<Uuid, u64>, AppError> {
        #[derive(FromQueryResult)]
        struct RoleCountRow {
            role_id: Uuid,
            cnt: i64,
        }

        // Count distinct users per role across all assignment scopes (global + category-scoped).
        let (sql, values) = Query::select()
            .column(user_roles::Column::RoleId)
            .expr_as(
                Expr::cust("COUNT(DISTINCT user_id)::BIGINT"),
                Alias::new("cnt"),
            )
            .from(user_roles::Entity)
            .and_where(not_expired())
            .group_by_col(user_roles::Column::RoleId)
            .build(PostgresQueryBuilder);

        let rows = RoleCountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| (r.role_id, r.cnt as u64)).collect())
    }

    async fn count_global_by_role(&self) -> Result<HashMap<Uuid, u64>, AppError> {
        #[derive(FromQueryResult)]
        struct RoleCountRow {
            role_id: Uuid,
            cnt: i64,
        }

        // Count distinct users per role for global assignments only (category_id IS NULL).
        let (sql, values) = Query::select()
            .column(user_roles::Column::RoleId)
            .expr_as(
                Expr::cust("COUNT(DISTINCT user_id)::BIGINT"),
                Alias::new("cnt"),
            )
            .from(user_roles::Entity)
            .and_where(user_roles::Column::CategoryId.is_null())
            .and_where(not_expired())
            .group_by_col(user_roles::Column::RoleId)
            .build(PostgresQueryBuilder);

        let rows = RoleCountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| (r.role_id, r.cnt as u64)).collect())
    }

}
