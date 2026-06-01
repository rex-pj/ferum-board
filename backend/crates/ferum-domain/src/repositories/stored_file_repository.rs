use async_trait::async_trait;
use uuid::Uuid;

use crate::AppError;

#[async_trait]
pub trait StoredFileRepository: Send + Sync {
    /// Returns true if a file with this exact CAS key already exists.
    async fn exists(&self, key: &str) -> Result<bool, AppError>;

    /// Insert a new file row. On conflict (same key), do nothing — file already exists.
    async fn upsert(
        &self,
        key: &str,
        content_type: &str,
        data: &[u8],
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    /// Atomically increment ref_count, return new count.
    async fn increment_ref(&self, key: &str) -> Result<i32, AppError>;

    /// Atomically decrement ref_count (floor 0), return new count.
    /// Returns 0 if the key does not exist.
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError>;

    /// Permanently delete the DB row (called by GC after ref_count hits 0).
    async fn delete_by_key(&self, key: &str) -> Result<(), AppError>;
}
