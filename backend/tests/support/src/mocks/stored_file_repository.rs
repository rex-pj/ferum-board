use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::AppError;

/// No-op StoredFileRepository — use when file storage is not the focus of the test.
pub struct NoopStoredFileRepository;

#[async_trait]
impl StoredFileRepository for NoopStoredFileRepository {
    async fn upsert_and_ref(&self, _key: &str, _ct: &str, _data: &[u8], _size: i64, _by: Option<Uuid>) -> Result<(), AppError> { Ok(()) }
    async fn decrement_ref(&self, _key: &str) -> Result<i32, AppError> { Ok(0) }
    async fn delete_by_key(&self, _key: &str) -> Result<(), AppError> { Ok(()) }
    async fn list_keys_with_prefix(&self, _prefix: &str) -> Result<Vec<String>, AppError> { Ok(vec![]) }
}
