use async_trait::async_trait;
use sea_orm::prelude::*;
use sea_orm::sea_query::{Expr, Func, OnConflict, PostgresQueryBuilder, Query};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::stored_files;
use ferum_application::shared::AppError;
use ferum_domain::repositories::stored_file_repository::{
    StoredFileRef, StoredFileRepository, UploadUsage,
};

/// CAS namespace for post attachments, re-exported from the use case that owns
/// the concept so this file holds no third copy of the literal.
///
/// It has to appear here at all because `attachment_keys_before` narrows on it in
/// SQL, and migration 000034's index is partial on the same prefix — if the two
/// spellings ever diverge the query silently stops using the index and
/// sequentially scans the table on every audit.
pub use ferum_application::usecases::storage_audit_usecase::ATTACHMENT_NAMESPACE as ATTACHMENT_KEY_PREFIX;

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
            // `NotSet` so the column DEFAULT fills it on insert and the conflict
            // path leaves it alone. This is the *referencing* path — nothing here
            // is staged, and moving `staged_at` would restart the grace period of
            // an attachment that a post is publishing rather than abandoning.
            staged_at: NotSet,
        })
        .on_conflict(
            OnConflict::column(stored_files::Column::Key)
                .value(
                    stored_files::Column::RefCount,
                    Expr::col((stored_files::Entity, stored_files::Column::RefCount)).add(1i32),
                )
                // **The conflict path must name the owner too.** Under database
                // storage `put` inserts this row first, so every upload lands
                // here rather than on the insert path — and while this clause was
                // absent, `uploaded_by_id` stayed NULL for every file ever
                // stored. Two things quietly stopped working: `usage_since`
                // filters on this column, so the per-account upload quota counted
                // zero and never triggered; and `resolve_stored_file` requires a
                // non-NULL owner, so no staged attachment was reachable even by
                // the person who uploaded it. Both are invisible under an object
                // store, where the row does not pre-exist.
                //
                // `COALESCE` and not an overwrite: CAS dedupes on content, so an
                // identical upload by a second user must not reassign the file.
                // First named uploader wins.
                .value(
                    stored_files::Column::UploadedById,
                    Expr::cust(
                        "COALESCE(stored_files.uploaded_by_id, EXCLUDED.uploaded_by_id)",
                    ),
                )
                .to_owned(),
        )
        .exec(&self.db)
        .await
        // Named rather than `?`: the statement carries a raw `COALESCE(...
        // EXCLUDED ...)` fragment, so the one thing worth knowing about a
        // failure here is what Postgres made of it.
        .map_err(|e| AppError::internal(format!("upsert_and_ref: {e}")))?;
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
            // `NotSet` so the column DEFAULT (`now()`) supplies it, matching the
            // `now()` in the conflict clause below. Both paths therefore read the
            // **database** clock — as does every other timestamp in this schema.
            // Setting it from `chrono::Utc::now()` here worked, but left one column
            // fed by two clocks, which is the kind of detail that costs an hour
            // during an incident.
            staged_at: NotSet,
        })
        // Updates **two** columns and nothing else. It used to be DO NOTHING, for
        // a good reason that this preserves: an existing key may already be
        // referenced by live posts, so re-staging must neither reset `ref_count`
        // to 0 nor bump it — and neither is touched here.
        //
        // What DO NOTHING also skipped was the owner. Under database storage
        // `put` inserts the row first, so staging always conflicts, and
        // `uploaded_by_id` stayed NULL forever — which made
        // `resolve_stored_file`'s ownership test unsatisfiable and left every
        // staged attachment 404 to its own uploader. `COALESCE` keeps the first
        // named owner, so a CAS-deduped re-upload cannot steal a file.
        //
        // **`staged_at` moves forward here, and unlike the owner it is NOT
        // COALESCEd.** That asymmetry is the whole point of the column. CAS keys
        // are content digests, so re-uploading a byte-identical image lands on an
        // existing row — possibly one whose original post was deleted years ago.
        // Both cleanup tools give an unreferenced attachment a grace period before
        // acting, and while they measured it from `created_at` this row read as
        // long-abandoned the instant it was re-staged: the sweep offered it for
        // manual deletion and the audit's apply pass released it, deleting the
        // image out of the composer that had just uploaded it.
        .on_conflict(
            OnConflict::column(stored_files::Column::Key)
                .value(
                    stored_files::Column::UploadedById,
                    Expr::cust(
                        "COALESCE(stored_files.uploaded_by_id, EXCLUDED.uploaded_by_id)",
                    ),
                )
                // `now()`, not `EXCLUDED.staged_at`. The latter would work —
                // `EXCLUDED` carries the defaults the proposed row would have got —
                // but it makes this clause's correctness depend on that Postgres
                // subtlety instead of stating the intent.
                .value(stored_files::Column::StagedAt, Expr::cust("now()"))
                .to_owned(),
        )
        .exec(&self.db)
        .await;

        // `RecordNotInserted` was the DO NOTHING signal and no longer arises,
        // but it stays accepted: it is the "key already existed" case, which is
        // success either way.
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

    async fn read_data(&self, key: &str) -> Result<Option<(Vec<u8>, String)>, AppError> {
        Ok(stored_files::Entity::find_by_id(key)
            .one(&self.db)
            .await?
            .and_then(|row| row.data.map(|bytes| (bytes, row.content_type))))
    }

    async fn clear_data(&self, key: &str) -> Result<(), AppError> {
        // `update_many` with a NULL expression rather than loading the row and
        // saving it back: the point of this call is to stop holding the bytes,
        // and a read-modify-write would pull them into memory to do it.
        stored_files::Entity::update_many()
            .col_expr(
                stored_files::Column::Data,
                Expr::value(sea_orm::Value::Bytes(None)),
            )
            .filter(stored_files::Column::Key.eq(key))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn referencing_pointers(&self) -> Result<Vec<String>, AppError> {
        // Raw SQL, and one statement rather than six: the six columns live in
        // five different aggregates, so no entity API expresses this, and a
        // round trip per column would be five more chances for the set to shift
        // underneath the audit.
        //
        // No user input reaches this — every fragment is a literal — so there is
        // nothing to parameterise.
        //
        // **Two shapes come back, and that is a property of the schema, not an
        // inconsistency to smooth over here.** The first four columns hold a
        // bare CAS key and carry a foreign key to `stored_files`; the last two
        // hold a `/files/{key}` URL and carry nothing. `key_from_url` declines a
        // bare key, so the caller falls back to the raw value — dropping it
        // instead would read every avatar and product image as unreferenced.
        //
        // Note which two lack the foreign key: `themes.preview_url` and
        // `site_config`. Those are exactly the two that leaked, and the FK is
        // why the others did not.
        const SQL: &str = "
            SELECT file_key AS ptr   FROM user_avatars
            UNION SELECT file_key    FROM user_covers
            UNION SELECT file_key    FROM thread_thumbnails
            UNION SELECT storage_key FROM product_media
            UNION SELECT preview_url FROM themes WHERE preview_url IS NOT NULL
            UNION SELECT value       FROM site_config
                     WHERE key IN ('logo_url', 'favicon_url') AND value <> ''
        ";
        // `e.to_string()` rather than `?`: `From<DbErr>` deliberately discards
        // the message so database internals never reach a response, but this
        // query is a string literal naming five tables, so the one thing an
        // operator needs from a failure here is *which column does not exist*.
        // `AppError::internal` is logged, not returned.
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                self.db.get_database_backend(),
                SQL.to_string(),
            ))
            .await
            .map_err(|e| AppError::internal(format!("referencing_pointers: {e}")))?;
        rows.into_iter()
            .map(|row| {
                row.try_get::<String>("", "ptr")
                    .map_err(|e| AppError::internal(format!("referencing_pointers row: {e}")))
            })
            .collect()
    }

    async fn attachment_keys_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, AppError> {
        // `LIKE 'post-attachments/%'` on the primary key, so this is an index
        // range scan rather than a table scan — the `%` is trailing.
        let keys = stored_files::Entity::find()
            .select_only()
            .column(stored_files::Column::Key)
            .filter(stored_files::Column::Key.starts_with(ATTACHMENT_KEY_PREFIX))
            // `StagedAt`, never `CreatedAt` — see the note on `upsert_staged`'s
            // conflict clause. A re-staged CAS key keeps its original
            // `created_at`, so filtering on that released images out of live
            // composers.
            .filter(stored_files::Column::StagedAt.lt(cutoff.fixed_offset()))
            .order_by_asc(stored_files::Column::Key)
            .into_tuple::<String>()
            .all(&self.db)
            .await?;
        Ok(keys)
    }

    async fn refs_for(&self, keys: &[String]) -> Result<Vec<StoredFileRef>, AppError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        // `select_only` for the same reason as `list_keys_with_prefix`: the full
        // model carries `data`, so hydrating it would pull every blob in the
        // batch across the wire to read four scalar columns.
        let rows = stored_files::Entity::find()
            .select_only()
            .column(stored_files::Column::Key)
            .column(stored_files::Column::RefCount)
            .column(stored_files::Column::CreatedAt)
            .column(stored_files::Column::StagedAt)
            .filter(stored_files::Column::Key.is_in(keys.iter().map(String::as_str)))
            // Leading `::`, and both halves spelled out. `sea_orm::prelude::*`
            // binds `DateTime` to chrono's *naive* type, so the bare name drops
            // the offset — and `sea_orm::*` also brings its own `chrono` into
            // scope, so even `chrono::DateTime` resolves to the naive one. Only
            // an absolute path reaches the real crate.
            .into_tuple::<(
                String,
                i32,
                ::chrono::DateTime<::chrono::FixedOffset>,
                ::chrono::DateTime<::chrono::FixedOffset>,
            )>()
            .all(&self.db)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(key, ref_count, created_at, staged_at)| StoredFileRef {
                key,
                ref_count,
                // Normalised here rather than passed on: `FixedOffset` must not
                // leave a repository.
                created_at: created_at.with_timezone(&::chrono::Utc),
                staged_at: staged_at.with_timezone(&::chrono::Utc),
            })
            .collect())
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
