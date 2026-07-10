use async_trait::async_trait;
use sea_orm::prelude::*;
use sea_orm::sea_query::{Expr, Func, OnConflict, PostgresQueryBuilder, Query};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::stored_files;
use ferum_application::shared::AppError;
use ferum_domain::repositories::stored_file_repository::{StoredFileRepository, UploadUsage};

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
    async fn usage_since(
        &self,
        uploaded_by_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<UploadUsage, AppError> {
        #[derive(FromQueryResult)]
        struct Row {
            file_count: i64,
            total_bytes: i64,
        }

        // SUM(bigint) yields `numeric` in Postgres, which will not deserialize
        // into i64 — cast both aggregates explicitly. COALESCE covers the
        // no-rows case, where SUM returns NULL rather than 0.
        let row = Row::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT COUNT(*)::bigint AS file_count, \
             COALESCE(SUM(size), 0)::bigint AS total_bytes \
             FROM stored_files \
             WHERE uploaded_by_id = $1 AND created_at >= $2",
            [uploaded_by_id.into(), since.fixed_offset().into()],
        ))
        .one(&self.db)
        .await?;

        Ok(row
            .map(|r| UploadUsage {
                file_count: r.file_count.max(0) as u64,
                total_bytes: r.total_bytes,
            })
            .unwrap_or(UploadUsage { file_count: 0, total_bytes: 0 }))
    }

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

    async fn upsert_staged(
        &self,
        key: &str,
        content_type: &str,
        data: &[u8],
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let res = stored_files::Entity::insert(stored_files::ActiveModel {
            key: Set(key.to_owned()),
            content_type: Set(content_type.to_owned()),
            data: Set(data.to_vec()),
            size: Set(size),
            ref_count: Set(0),
            uploaded_by_id: Set(uploaded_by_id),
            created_at: NotSet,
        })
        // DO NOTHING, not DO UPDATE: an existing key may already be referenced
        // by live posts (CAS dedupes identical bytes), and re-staging it must
        // neither reset its ref_count to 0 nor bump it.
        .on_conflict(
            OnConflict::column(stored_files::Column::Key)
                .do_nothing()
                .to_owned(),
        )
        .exec(&self.db)
        .await;

        // sea-orm surfaces a no-op DO NOTHING as RecordNotInserted rather than
        // Ok — which is exactly the "key already existed" case we want to allow.
        match res {
            Ok(_) | Err(DbErr::RecordNotInserted) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn increment_ref(&self, key: &str) -> Result<(), AppError> {
        stored_files::Entity::update_many()
            .col_expr(
                stored_files::Column::RefCount,
                Expr::col(stored_files::Column::RefCount).add(1i32),
            )
            .filter(stored_files::Column::Key.eq(key))
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
