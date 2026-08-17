use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::repositories::stored_file_repository::{
    StoredFileRef, StoredFileRepository, UploadUsage,
};
use ferum_domain::AppError;

/// No-op StoredFileRepository — use when file storage is not the focus of the test.
/// Reports zero usage, so quota checks always pass.
pub struct NoopStoredFileRepository;

#[async_trait]
impl StoredFileRepository for NoopStoredFileRepository {
    async fn referencing_pointers(&self) -> Result<Vec<String>, AppError> { Ok(vec![]) }
    async fn attachment_keys_before(&self, _cutoff: chrono::DateTime<chrono::Utc>) -> Result<Vec<String>, AppError> { Ok(vec![]) }
    async fn usage_since(&self, _by: Uuid, _since: chrono::DateTime<chrono::Utc>) -> Result<UploadUsage, AppError> {
        Ok(UploadUsage { file_count: 0, total_bytes: 0 })
    }
    async fn upsert_and_ref(&self, _key: &str, _ct: &str, _size: i64, _by: Option<Uuid>) -> Result<(), AppError> { Ok(()) }
    async fn upsert_staged(&self, _key: &str, _ct: &str, _size: i64, _by: Option<Uuid>) -> Result<(), AppError> { Ok(()) }
    async fn increment_ref(&self, _key: &str) -> Result<(), AppError> { Ok(()) }
    /// `None` = "nothing staged here", which makes attachment promotion a no-op
    /// for every test that is not about promotion.
    async fn read_data(&self, _key: &str) -> Result<Option<(Vec<u8>, String)>, AppError> { Ok(None) }
    async fn clear_data(&self, _key: &str) -> Result<(), AppError> { Ok(()) }
    async fn decrement_ref(&self, _key: &str) -> Result<i32, AppError> { Ok(0) }
    async fn delete_if_unreferenced(&self, _key: &str) -> Result<bool, AppError> { Ok(true) }
    /// Empty = "no row for any of these", which reads as *every* key being an
    /// orphan. Correct for a repository that stores nothing, and the sweep tests
    /// bring their own double rather than relying on it.
    async fn refs_for(&self, _: &[String]) -> Result<Vec<StoredFileRef>, AppError> { Ok(vec![]) }
    async fn list_keys_with_prefix(&self, _prefix: &str) -> Result<Vec<String>, AppError> { Ok(vec![]) }
}
