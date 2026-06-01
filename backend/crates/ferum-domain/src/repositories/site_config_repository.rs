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
