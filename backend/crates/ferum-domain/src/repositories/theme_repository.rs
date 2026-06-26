use async_trait::async_trait;

use crate::models::theme::{NewTheme, Theme};
use crate::AppError;

#[async_trait]
pub trait ThemeRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Theme>, AppError>;
    async fn get_active(&self) -> Result<Theme, AppError>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Theme>, AppError>;
    async fn upsert(&self, theme: NewTheme) -> Result<Theme, AppError>;
    async fn set_active(&self, slug: &str) -> Result<(), AppError>;
    async fn update_preview_url(&self, slug: &str, url: Option<String>) -> Result<(), AppError>;
    async fn delete(&self, slug: &str) -> Result<(), AppError>;
}
