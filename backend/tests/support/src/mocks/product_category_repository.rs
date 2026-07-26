use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

use ferum_domain::models::product_category::{
    NewProductCategory, ProductCategory, UpdateProductCategory,
};
use ferum_domain::repositories::product_repository::{
    AutoAssignReport, ProductCategoryRepository,
};
use ferum_domain::AppError;

mockall::mock! {
    pub ProductCategoryRepository {}

    #[async_trait]
    impl ProductCategoryRepository for ProductCategoryRepository {
        async fn list(&self) -> Result<Vec<ProductCategory>, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<ProductCategory>, AppError>;
        async fn create(&self, category: NewProductCategory) -> Result<ProductCategory, AppError>;
        async fn update(
            &self,
            id: Uuid,
            patch: UpdateProductCategory,
        ) -> Result<ProductCategory, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
        async fn product_counts(&self) -> Result<HashMap<Option<Uuid>, u64>, AppError>;
        async fn auto_assign_categories(&self, dry_run: bool) -> Result<AutoAssignReport, AppError>;
    }
}

/// Empty taxonomy. For tests that construct a `ProductUseCase` but never touch
/// categories — a mock with no expectations would panic on the first call, and
/// several product tests list categories only incidentally.
pub struct NoopProductCategoryRepository;

#[async_trait]
impl ProductCategoryRepository for NoopProductCategoryRepository {
    async fn list(&self) -> Result<Vec<ProductCategory>, AppError> {
        Ok(vec![])
    }
    async fn find_by_id(&self, _id: Uuid) -> Result<Option<ProductCategory>, AppError> {
        Ok(None)
    }
    async fn create(&self, _category: NewProductCategory) -> Result<ProductCategory, AppError> {
        Err(AppError::NotFound)
    }
    async fn update(
        &self,
        _id: Uuid,
        _patch: UpdateProductCategory,
    ) -> Result<ProductCategory, AppError> {
        Err(AppError::NotFound)
    }
    async fn delete(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn product_counts(&self) -> Result<HashMap<Option<Uuid>, u64>, AppError> {
        Ok(HashMap::new())
    }
    async fn auto_assign_categories(&self, _dry_run: bool) -> Result<AutoAssignReport, AppError> {
        Ok(AutoAssignReport::default())
    }
}
