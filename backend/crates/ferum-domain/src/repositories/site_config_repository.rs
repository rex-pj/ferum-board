use std::collections::HashMap;

use async_trait::async_trait;

use crate::AppError;

#[async_trait]
pub trait SiteConfigRepository: Send + Sync {
    async fn get_all(&self) -> Result<HashMap<String, String>, AppError>;
    async fn get(&self, key: &str) -> Result<Option<String>, AppError>;
    async fn set(&self, key: &str, value: &str) -> Result<(), AppError>;
    async fn set_many(&self, entries: &HashMap<String, String>) -> Result<(), AppError>;
}

/// Read a key from a site_config repository, parse it as `u64`, and fall back to `default`
/// when the key is absent, empty, or not a valid integer. Errors from the repository are
/// silently swallowed so a DB hiccup never blocks a user action.
pub async fn get_config_u64(repo: &dyn SiteConfigRepository, key: &str, default: u64) -> u64 {
    repo.get(key).await.ok().flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub async fn get_config_i64(repo: &dyn SiteConfigRepository, key: &str, default: i64) -> i64 {
    repo.get(key).await.ok().flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub async fn get_config_i32(repo: &dyn SiteConfigRepository, key: &str, default: i32) -> i32 {
    repo.get(key).await.ok().flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
