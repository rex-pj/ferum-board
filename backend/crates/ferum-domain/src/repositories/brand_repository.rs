use async_trait::async_trait;
use uuid::Uuid;

use crate::models::brand::{Brand, NewBrand, UpdateBrand};
use crate::AppError;

#[async_trait]
pub trait BrandRepository: Send + Sync {
    async fn create(&self, brand: NewBrand) -> Result<Brand, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Brand>, AppError>;
    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Brand>, AppError>;
    /// List every brand, ordered by name (reference data — small, unpaginated).
    async fn list(&self) -> Result<Vec<Brand>, AppError>;
    async fn update(&self, id: Uuid, patch: UpdateBrand) -> Result<Brand, AppError>;
    /// Delete a brand. Referencing products have their `brand_id` set to NULL.
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;
}
