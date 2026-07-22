use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

use ferum_domain::models::product::{NewProduct, Product};
use ferum_domain::models::product_media::{NewProductMedia, ProductMedia};
use ferum_domain::repositories::product_repository::{
    ProductDependents, ProductListFilter, ProductListItem, ProductRepository, UpdateProduct,
};
use ferum_domain::AppError;

mockall::mock! {
    pub ProductRepository {}

    #[async_trait]
    impl ProductRepository for ProductRepository {
        async fn create(&self, product: NewProduct) -> Result<Product, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Product>, AppError>;
        async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Product>, AppError>;
        async fn list(
            &self,
            filter: ProductListFilter,
            page: u64,
            per_page: u64,
        ) -> Result<(Vec<ProductListItem>, u64), AppError>;
        async fn update(&self, id: Uuid, patch: UpdateProduct) -> Result<Product, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
        async fn count_dependents(&self, product_id: Uuid) -> Result<ProductDependents, AppError>;
        // Slice params carry no explicit lifetime here, matching the trait — an
        // added `'a` makes mockall generate a signature the borrow checker reads
        // as self and the slice sharing a lifetime.
        async fn set_materials(
            &self,
            product_id: Uuid,
            material_ids: &[Uuid],
        ) -> Result<(), AppError>;
        async fn list_material_ids(&self, product_id: Uuid) -> Result<Vec<Uuid>, AppError>;
        async fn primary_image_by_threads(
            &self,
            thread_ids: &[Uuid],
        ) -> Result<HashMap<Uuid, String>, AppError>;
        async fn add_media(&self, media: NewProductMedia) -> Result<ProductMedia, AppError>;
        async fn list_media(&self, product_id: Uuid) -> Result<Vec<ProductMedia>, AppError>;
        async fn find_media(&self, media_id: Uuid) -> Result<Option<ProductMedia>, AppError>;
        async fn delete_media(&self, media_id: Uuid) -> Result<(), AppError>;
    }
}
