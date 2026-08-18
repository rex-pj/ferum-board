use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use crate::m20260001_000013_create_stored_files::StoredFiles;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000034_add_staged_at_to_stored_files"
    }
}

/// When a key was last *staged*, as distinct from when its bytes first landed.
///
/// `created_at` cannot answer that question and the difference is not academic.
/// CAS keys are content digests, so re-uploading a byte-identical image reuses the
/// existing row — `upsert_staged` takes its `ON CONFLICT` path and leaves
/// `created_at` alone. An attachment whose original post was deleted years ago is
/// therefore indistinguishable, by `created_at`, from one a member has open in a
/// composer right now.
///
/// Both cleanup tools grant unreferenced attachments a grace period before acting,
/// and both were reading `created_at` — so the sweep offered such a key for manual
/// deletion and the audit's apply pass **released** it, deleting the image out of a
/// live draft. This column is what the grace period is supposed to have been
/// measuring all along.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── Step 1: add it nullable and WITHOUT a default ────────────────────
        //
        // The missing default is the point. Postgres 11+ fills every existing row
        // when `ADD COLUMN` carries one, so declaring it here would stamp `now()`
        // across the whole table — hiding every genuinely abandoned attachment
        // from both cleanup tools for a day. Existing rows must land NULL so step
        // 3 can tell them apart from rows written after this migration started.
        manager
            .alter_table(
                Table::alter()
                    .table(StoredFiles::Table)
                    .add_column(
                        ColumnDef::new(StoredFiles::StagedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        // ── Step 2: the default, immediately ─────────────────────────────────
        //
        // **This must not wait for the `NOT NULL` below.** Migrations run at
        // startup while the previous instance is still serving, so uploads keep
        // arriving throughout — and every insert path leaves `staged_at` to the
        // column default (`DatabaseStorageService::put` builds its ActiveModel
        // with `..Default::default()`, so it does not even mention the column).
        // With the default set only at the end, any upload landing inside this
        // window writes NULL, step 4 then fails, and the deploying instance
        // refuses to start. Timing-dependent, so it passes everywhere except a
        // busy production rollout.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE stored_files ALTER COLUMN staged_at SET DEFAULT now()")
            .await?;

        // ── Step 3: backfill only the pre-existing rows ──────────────────────
        //
        // `WHERE staged_at IS NULL` is load-bearing now that step 2 exists: an
        // unqualified UPDATE would overwrite a row inserted seconds ago — whose
        // `staged_at` is correctly `now()` — with its much older `created_at`,
        // handing a live upload straight to the cleanup tools.
        //
        // For a row that predates this column, `created_at` is the best available
        // lower bound on when it was staged, and it is what the tools were reading
        // anyway. `data` is TOASTed and untouched here, so this rewrites heap
        // tuples only — cost is per row, not per byte. Do not batch it.
        manager
            .get_connection()
            .execute_unprepared("UPDATE stored_files SET staged_at = created_at WHERE staged_at IS NULL")
            .await?;

        // ── Step 4: now it can be NOT NULL ───────────────────────────────────
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE stored_files ALTER COLUMN staged_at SET NOT NULL")
            .await?;

        // `attachment_keys_before` scans this column over the `post-attachments/`
        // key range on every audit. Partial, because no other namespace is ever
        // filtered by it.
        //
        // A partial index only gets used when the query predicate *implies* the
        // index predicate, so the prefix here and the one in that query must stay
        // identical — the query narrows on `ATTACHMENT_KEY_PREFIX` for exactly that
        // reason. `the_age_floor_query_actually_uses_its_partial_index` runs
        // `EXPLAIN` and asserts the age bound lands as an `Index Cond`, not a
        // per-row `Filter`; both degradations are silent and cost a full scan of
        // the namespace on every audit.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_stored_files_staged_at \
                 ON stored_files(staged_at) \
                 WHERE key LIKE 'post-attachments/%'",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_stored_files_staged_at")
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(StoredFiles::Table)
                    .drop_column(StoredFiles::StagedAt)
                    .to_owned(),
            )
            .await
    }
}
