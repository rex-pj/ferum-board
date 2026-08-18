use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::AppError;

/// Rolling-window upload usage for one account, backing the per-user quota.
pub struct UploadUsage {
    pub file_count: u64,
    pub total_bytes: i64,
}

/// One row's reference bookkeeping, without its bytes.
///
/// `data` is deliberately absent: every caller wants to reason about references,
/// and hydrating the full model would pull each blob across the wire to read two
/// scalars.
pub struct StoredFileRef {
    pub key: String,
    pub ref_count: i32,
    /// When these **bytes** first landed — not when this key was last staged.
    /// CAS dedupes on content, so re-uploading an identical image keeps the
    /// original timestamp. Treat it as a floor on age, never as "last touched".
    pub created_at: DateTime<Utc>,
    /// When this key was last staged, which is what an attachment's grace period
    /// must be measured against.
    ///
    /// **Never substitute `created_at` here.** A byte-identical re-upload reuses
    /// the existing row, so a key whose original post was deleted long ago is
    /// indistinguishable by `created_at` from one a member has open in a composer
    /// — and both cleanup tools treat "old and unreferenced" as actionable.
    pub staged_at: DateTime<Utc>,
}

#[async_trait]
pub trait StoredFileRepository: Send + Sync {
    /// Files this user uploaded at or after `since`. Counts every CAS namespace
    /// (avatar, cover, thumbnail, attachment) so the quota is a single storage
    /// budget per account rather than a per-feature allowance that can be
    /// summed to bypass it.
    ///
    /// Note this attributes a file to whoever *first* uploaded those exact
    /// bytes: a CAS key that already exists is only ref-counted, not re-inserted,
    /// so a second uploader of identical content consumes no new storage and is
    /// correctly not charged for it.
    async fn usage_since(
        &self,
        uploaded_by_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<UploadUsage, AppError>;

    /// Atomically insert a new file row (ref_count=1) or, if the key already
    /// exists, increment its ref_count — all in a single SQL statement.
    ///
    /// Records *metadata* only. The bytes are written separately, by whichever
    /// `StorageService` is configured, and callers must do that first: this row
    /// is what makes a key discoverable, so creating it before the content
    /// exists opens a window where a reader can be handed a URL to nothing.
    async fn upsert_and_ref(
        &self,
        key: &str,
        content_type: &str,
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    /// Insert a *staged* row with `ref_count = 0` — content that exists in
    /// storage but is not yet referenced by any post. If the key already exists
    /// its ref_count is left untouched (identical bytes may already be
    /// referenced by someone else's post; re-uploading must not inflate that).
    ///
    /// Staged rows are not publicly servable — see the `/files/:key` gate.
    /// They become public only once `increment_ref` is called for them by a
    /// post that actually embeds the URL.
    async fn upsert_staged(
        &self,
        key: &str,
        content_type: &str,
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    /// Atomically increment ref_count for an existing key. A key that does not
    /// exist is a no-op, not an error: post content may reference an arbitrary
    /// `/files/...` URL that was never uploaded here.
    async fn increment_ref(&self, key: &str) -> Result<(), AppError>;

    /// Atomically decrement ref_count (floor 0), return new count.
    /// Returns 0 if the key does not exist.
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError>;

    /// Reads back bytes held in the row, with their content type.
    ///
    /// Exists for one caller: promoting a post attachment out of database
    /// staging once a post publishes it. `None` when the row is absent or its
    /// bytes already live in an object store.
    async fn read_data(&self, key: &str) -> Result<Option<(Vec<u8>, String)>, AppError>;

    /// Drops the bytes from the row, keeping the metadata and `ref_count`.
    ///
    /// The second half of promotion, and **it must run after** the object-store
    /// write has succeeded, never before: `data` is the only copy until that
    /// write lands, so clearing first turns a failed upload into permanent data
    /// loss. Same ordering rule as "bytes before row" on the way in.
    async fn clear_data(&self, key: &str) -> Result<(), AppError>;

    // `delete_by_key` used to live here — an unconditional row delete. It was
    // removed rather than deprecated: it deleted the row and nothing else, so
    // under any object store it stranded the bytes AND destroyed the only record
    // that could have found them again. Three call sites used it that way. Use
    // `delete_if_unreferenced` (via `ForumJob::GcStorageKey`) instead, which
    // deletes both and re-checks the count while holding the row lock.

    /// Deletes the row only if still unreferenced. **The `ref_count = 0` test
    /// must stay inside the DELETE**: GC is asynchronous and CAS dedupes on
    /// content, so an identical upload can revive the row before the job runs.
    /// Read-then-delete cannot see that and destroys the new reference.
    ///
    /// `false` means revived or already gone — leave the blob alone.
    async fn delete_if_unreferenced(&self, key: &str) -> Result<bool, AppError>;

    /// Reference bookkeeping for whichever of `keys` have a row.
    ///
    /// Answers every half of the orphan sweep in one query: a key the store holds
    /// but this omits has no row at all (an orphaned object), one it returns at
    /// `<= 0` has a row whose collection never happened, and `created_at`
    /// separates an attachment nobody will ever publish from one a composer is
    /// still holding open. Asking per key instead would be a round trip per
    /// object in the bucket.
    ///
    /// Absent keys are simply missing from the result — the caller compares
    /// against what it asked for rather than expecting a placeholder.
    async fn refs_for(&self, keys: &[String]) -> Result<Vec<StoredFileRef>, AppError>;

    /// Every value that currently points at a stored file, across the columns
    /// that hold one.
    ///
    /// Returned **raw**, as stored, in the two shapes the schema actually uses:
    /// `user_avatars`, `user_covers`, `thread_thumbnails` and `product_media`
    /// hold a bare CAS key (and a foreign key to this table); `themes.preview_url`
    /// and `site_config` hold a `/files/{key}` URL and no constraint at all.
    /// Those last two are exactly the ones that leaked.
    ///
    /// Resolved by the caller through `StorageService::key_from_url`, falling
    /// back to the raw value when it declines — which it does for a bare key.
    /// That split is not indirection for its own sake: the URL shape varies by
    /// backend, and rows written before `ports::file_url` existed can still hold
    /// an absolute `{cdn}/files/{key}`. Matching in SQL with `LIKE` would miss
    /// exactly those, and since the caller's next move is releasing whatever it
    /// did not find, a miss deletes a live image.
    ///
    /// Soft-deleted rows count as references — the row still exists and still
    /// names the file, so a removed thread's thumbnail is not garbage.
    ///
    /// Covers the six namespaces with an indexable owner. `post-attachments/`
    /// lives inside post markdown and `plugin_*/` inside plugin-authored markup;
    /// neither has a column to read, so neither is audited.
    async fn referencing_pointers(&self) -> Result<Vec<String>, AppError>;

    /// Post-attachment keys whose row was created before `cutoff`.
    ///
    /// **The age floor is load-bearing, not tidiness.** A staged attachment
    /// seconds old belongs to a composer somebody still has open — it is
    /// referenced by nothing yet *and never will be until they hit post*.
    /// Reporting it invites deleting an image out of a live draft.
    async fn attachment_keys_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, AppError>;

    /// List keys starting with `prefix` — used at plugin uninstall to find every
    /// file it ever uploaded (keys are namespaced `plugin_{slug}/...` by cas_key)
    /// so they can be dereferenced instead of orphaned forever.
    async fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, AppError>;
}
