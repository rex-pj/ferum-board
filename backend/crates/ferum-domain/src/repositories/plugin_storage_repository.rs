use async_trait::async_trait;
use uuid::Uuid;

use crate::AppError;

/// Durable, plugin-scoped key/value storage — the persistent counterpart to
/// `CacheService` (which is TTL-bound and may be evicted). Every operation is
/// namespaced by `plugin_id` at the repository layer, so a plugin can never
/// read or write another plugin's keys regardless of what key string it passes.
#[async_trait]
pub trait PluginStorageRepository: Send + Sync {
    async fn get(&self, plugin_id: Uuid, key: &str) -> Result<Option<serde_json::Value>, AppError>;
    async fn set(&self, plugin_id: Uuid, key: &str, value: serde_json::Value) -> Result<(), AppError>;
    async fn delete(&self, plugin_id: Uuid, key: &str) -> Result<(), AppError>;
    /// List up to `limit` keys under `plugin_id` whose key starts with `prefix`
    /// (empty prefix matches all), ordered lexicographically.
    async fn list_keys(&self, plugin_id: Uuid, prefix: &str, limit: u64) -> Result<Vec<String>, AppError>;
}
