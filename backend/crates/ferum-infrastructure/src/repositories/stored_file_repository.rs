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
                // `Ord::max`, not `ExprTrait::max` — sea-query 1.0's ExprTrait is
                // blanket-implemented, so a bare `.max()` on an i64 is ambiguous
                // wherever `sea_orm::*` is glob-imported.
                file_count: Ord::max(r.file_count, 0) as u64,
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
                    Expr::val(0i32),
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
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        stored_files::Entity::insert(stored_files::ActiveModel {
            key: Set(key.to_owned()),
            content_type: Set(content_type.to_owned()),
            // `NotSet`, never `Set(None)`: the database backend's `put` has
            // already written the bytes into this very row, so naming the column
            // here at all would overwrite them with NULL on the conflict path.
            // Under S3 the row is new and `data` stays NULL, which is what the
            // nullable column exists for.
            data: NotSet,
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
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let res = stored_files::Entity::insert(stored_files::ActiveModel {
            key: Set(key.to_owned()),
            content_type: Set(content_type.to_owned()),
            // See `upsert_and_ref` — the bytes are the storage backend's to write.
            data: NotSet,
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

    async fn delete_if_unreferenced(&self, key: &str) -> Result<bool, AppError> {
        // The `ref_count = 0` predicate belongs in the DELETE, not in a
        // preceding SELECT — see the trait doc. Postgres evaluates it while
        // holding the row lock, so a concurrent `upsert_and_ref` either lands
        // first (and this deletes nothing) or lands after (and finds no row to
        // revive, so it inserts a fresh one). Neither order loses a reference.
        let res = stored_files::Entity::delete_many()
            .filter(stored_files::Column::Key.eq(key))
            .filter(stored_files::Column::RefCount.lte(0))
            .exec(&self.db)
            .await?;
        Ok(res.rows_affected > 0)
    }

    async fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, AppError> {
        // Escape LIKE metacharacters so a prefix containing "%"/"_" (e.g. an
        // unusual plugin slug) can't widen the scan beyond its own namespace.
        let escaped = prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        // `select_only().column(Key)` matters here far more than usual: hydrating
        // the full model would also fetch `data`, the file's entire byte content.
        // The one caller that matters is plugin uninstall, which walks every blob
        // in a `plugin_{slug}/` namespace — so the old form read every uploaded
        // file into memory purely to learn its name.
        Ok(stored_files::Entity::find()
            .select_only()
            .column(stored_files::Column::Key)
            .filter(stored_files::Column::Key.like(format!("{escaped}%")))
            .into_tuple::<String>()
            .all(&self.db)
            .await?)
    }
}
