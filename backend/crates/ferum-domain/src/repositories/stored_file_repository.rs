use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::AppError;

/// Rolling-window upload usage for one account, backing the per-user quota.
pub struct UploadUsage {
    pub file_count: u64,
    pub total_bytes: i64,
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

    /// Permanently delete the DB row (called by GC after ref_count hits 0).
    async fn delete_by_key(&self, key: &str) -> Result<(), AppError>;

    /// Delete the row only if it is still unreferenced, reporting whether it
    /// went. The `ref_count = 0` test lives inside the DELETE on purpose.
    ///
    /// GC is asynchronous: a key is enqueued once its count reaches 0, but CAS
    /// deduplicates on content, so an upload of the identical bytes in the gap
    /// before the job runs legitimately revives the row at count 1. Reading the
    /// count and then deleting cannot see that — the row must be re-tested in
    /// the same statement that removes it, or GC destroys the reference the new
    /// uploader just took.
    ///
    /// Returns `false` when the row was revived or already gone, in which case
    /// the caller must leave the underlying blob alone.
    async fn delete_if_unreferenced(&self, key: &str) -> Result<bool, AppError>;

    /// List keys starting with `prefix` — used at plugin uninstall to find every
    /// file it ever uploaded (keys are namespaced `plugin_{slug}/...` by cas_key)
    /// so they can be dereferenced instead of orphaned forever.
    async fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, AppError>;
}
