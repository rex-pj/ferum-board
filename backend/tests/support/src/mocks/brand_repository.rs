use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::brand::{Brand, NewBrand, UpdateBrand};
use ferum_domain::repositories::brand_repository::BrandRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub BrandRepository {}

    #[async_trait]
    impl BrandRepository for BrandRepository {
        async fn create(&self, brand: NewBrand) -> Result<Brand, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Brand>, AppError>;
        async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Brand>, AppError>;
        async fn list(&self) -> Result<Vec<Brand>, AppError>;
        async fn update(&self, id: Uuid, patch: UpdateBrand) -> Result<Brand, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
    }
}
