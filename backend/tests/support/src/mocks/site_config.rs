use std::collections::HashMap;

use async_trait::async_trait;

use ferum_domain::repositories::site_config_repository::SiteConfigRepository;
use ferum_domain::AppError;

/// In-memory SiteConfigRepository for tests. Avoids mockall &str lifetime issues.
pub struct InMemorySiteConfig {
    data: HashMap<String, String>,
}

impl InMemorySiteConfig {
    pub fn empty() -> Self {
        Self { data: HashMap::new() }
    }

    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.data.insert(key.to_string(), value.to_string());
        self
    }
}

#[async_trait]
impl SiteConfigRepository for InMemorySiteConfig {
    async fn get_all(&self) -> Result<HashMap<String, String>, AppError> {
        Ok(self.data.clone())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        Ok(self.data.get(key).cloned())
    }

    async fn set(&self, _key: &str, _value: &str) -> Result<(), AppError> {
        Ok(())
    }

    async fn set_many(&self, _entries: &HashMap<String, String>) -> Result<(), AppError> {
        Ok(())
    }
}
