use async_trait::async_trait;
use uuid::Uuid;

use crate::models::material::{Material, NewMaterial, UpdateMaterial};
use crate::AppError;

#[async_trait]
pub trait MaterialRepository: Send + Sync {
    async fn create(&self, material: NewMaterial) -> Result<Material, AppError>;
    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Material>, AppError>;
    /// List materials, optionally filtered by category (e.g. "wood_natural").
    async fn list<'a>(&self, category: Option<&'a str>) -> Result<Vec<Material>, AppError>;
    async fn update(&self, id: Uuid, patch: UpdateMaterial) -> Result<Material, AppError>;
    /// Delete a material. `product_materials` links cascade away automatically.
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;
}
