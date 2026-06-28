use std::time::Duration;

use async_trait::async_trait;

use ferum_application::ports::CacheService;
use ferum_application::shared::AppError;

mockall::mock! {
    pub CacheService {}

    #[async_trait]
    impl CacheService for CacheService {
        async fn get(&self, key: &str) -> Option<String>;
        async fn set<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<(), AppError>;
        async fn set_nx<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<bool, AppError>;
        async fn del(&self, key: &str) -> Result<(), AppError>;
        async fn del_prefix(&self, prefix: &str) -> Result<(), AppError>;
        async fn incr_with_ttl(&self, key: &str, ttl: Duration) -> Result<u64, AppError>;
        async fn exists(&self, key: &str) -> bool;
    }
}

/// No-op CacheService — always cache miss, all writes succeed silently.
pub struct NoopCacheService;

#[async_trait]
impl CacheService for NoopCacheService {
    async fn get(&self, _key: &str) -> Option<String> { None }
    async fn set<'a>(&self, _key: &'a str, _value: &'a str, _ttl: Duration) -> Result<(), AppError> { Ok(()) }
    async fn set_nx<'a>(&self, _key: &'a str, _value: &'a str, _ttl: Duration) -> Result<bool, AppError> { Ok(true) }
    async fn del(&self, _key: &str) -> Result<(), AppError> { Ok(()) }
    async fn del_prefix(&self, _prefix: &str) -> Result<(), AppError> { Ok(()) }
    async fn incr_with_ttl(&self, _key: &str, _ttl: Duration) -> Result<u64, AppError> { Ok(1) }
    async fn exists(&self, _key: &str) -> bool { false }
}
