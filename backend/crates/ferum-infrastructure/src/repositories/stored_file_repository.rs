use async_trait::async_trait;
use sea_orm::prelude::*;
use sea_orm::sea_query::{Expr, Func, OnConflict, PostgresQueryBuilder, Query};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::stored_files;
use ferum_application::shared::AppError;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;

pub struct PgStoredFileRepository {
    db: DatabaseConnection,
}

impl PgStoredFileRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl StoredFileRepository for PgStoredFileRepository {
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError> {
        #[derive(FromQueryResult)]
        struct Row {
            ref_count: i32,
        }

        let (sql, values) = Query::update()
            .table(stored_files::Entity)
            .value(
                stored_files::Column::RefCount,
                Func::greatest([
                    Expr::col(stored_files::Column::RefCount).sub(1i32),
                    Expr::val(0i32).into(),
                ]),
            )
            .and_where(stored_files::Column::Key.eq(key))
            .returning_col(stored_files::Column::RefCount)
            .build(PostgresQueryBuilder);

        let row = Row::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .one(&self.db)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(row.map(|r| r.ref_count).unwrap_or(0))
    }

    async fn upsert_and_ref(
        &self,
        key: &str,
        content_type: &str,
        data: &[u8],
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        stored_files::Entity::insert(stored_files::ActiveModel {
            key: Set(key.to_owned()),
            content_type: Set(content_type.to_owned()),
            data: Set(data.to_vec()),
            size: Set(size),
            ref_count: Set(1),
            uploaded_by_id: Set(uploaded_by_id),
            created_at: NotSet,
        })
        .on_conflict(
            OnConflict::column(stored_files::Column::Key)
                .value(
                    stored_files::Column::RefCount,
                    Expr::col((stored_files::Entity, stored_files::Column::RefCount)).add(1i32),
                )
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        Ok(())
    }

    async fn delete_by_key(&self, key: &str) -> Result<(), AppError> {
        stored_files::Entity::delete_by_id(key)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, AppError> {
        // Escape LIKE metacharacters so a prefix containing "%"/"_" (e.g. an
        // unusual plugin slug) can't widen the scan beyond its own namespace.
        let escaped = prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        Ok(stored_files::Entity::find()
            .filter(stored_files::Column::Key.like(format!("{escaped}%")))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| m.key)
            .collect())
    }
}
