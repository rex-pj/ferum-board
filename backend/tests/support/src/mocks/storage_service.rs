use async_trait::async_trait;
use bytes::Bytes;

use ferum_application::ports::{StorageService, FILES_PREFIX};
use ferum_domain::AppError;

/// No-op `StorageService` for tests where blob bytes are not the subject.
///
/// Mirrors the same-origin URL shape of `DatabaseStorageService`, which is what
/// the assertions in the use-case tests were written against. A double that
/// invented its own shape would quietly decouple those tests from the behaviour
/// they are meant to pin.
pub struct NoopStorageService;

#[async_trait]
impl StorageService for NoopStorageService {
    async fn put(&self, _key: &str, _data: Bytes, _content_type: &str) -> Result<(), AppError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<(), AppError> {
        Ok(())
    }

    fn public_url(&self, key: &str) -> String {
        format!("{FILES_PREFIX}{key}")
    }

    fn key_from_url(&self, url: &str) -> Option<String> {
        url.rfind(FILES_PREFIX)
            .map(|i| url[i + FILES_PREFIX.len()..].to_string())
            .filter(|k| !k.is_empty())
    }
}
