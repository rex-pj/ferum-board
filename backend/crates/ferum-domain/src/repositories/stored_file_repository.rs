use async_trait::async_trait;
use uuid::Uuid;

use crate::AppError;

#[async_trait]
pub trait StoredFileRepository: Send + Sync {
    /// Atomically insert a new file row (ref_count=1) or, if the key already
    /// exists, increment its ref_count — all in a single SQL statement.
    async fn upsert_and_ref(
        &self,
        key: &str,
        content_type: &str,
        data: &[u8],
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    /// Atomically decrement ref_count (floor 0), return new count.
    /// Returns 0 if the key does not exist.
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError>;

    /// Permanently delete the DB row (called by GC after ref_count hits 0).
    async fn delete_by_key(&self, key: &str) -> Result<(), AppError>;

    /// List keys starting with `prefix` — used at plugin uninstall to find every
    /// file it ever uploaded (keys are namespaced `plugin_{slug}/...` by cas_key)
    /// so they can be dereferenced instead of orphaned forever.
    async fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, AppError>;
}
