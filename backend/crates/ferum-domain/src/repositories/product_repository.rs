use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

use crate::models::product::{NewProduct, Product, ProductStatus, ProductType};
use crate::models::product_media::{NewProductMedia, ProductMedia};
use crate::AppError;

/// Ordering for catalog listing. On a review platform the default browse order
/// should still be "newest", but users need to sort by quality signals too.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ProductSort {
    #[default]
    Newest,
    TopRated,
    MostReviewed,
}

/// Filters for catalog browsing. All fields are optional (AND-combined).
#[derive(Debug, Default, Clone)]
pub struct ProductListFilter {
    pub product_type: Option<ProductType>,
    pub status: Option<ProductStatus>,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub material_id: Option<Uuid>,
    pub query: Option<String>,
    pub sort: ProductSort,
    /// When set alongside a `status` filter, also include products created by
    /// this user regardless of status — so a contributor can still find their
    /// own pending submissions in the public list. `None` = strict status only.
    pub include_own: Option<Uuid>,
}

/// A product enriched with its aggregate rating, for catalog cards. The average
/// is exact `numeric(4,2)` (no float jitter); `review_count` is 0 when unrated.
#[derive(Debug, Clone)]
pub struct ProductListItem {
    pub product: Product,
    pub review_count: i32,
    pub avg_overall: Option<Decimal>,
}

/// What a product would take down with it. Gathered before a hard delete so the
/// caller can refuse (or warn) instead of silently orphaning review threads:
/// `threads.product_id` is `ON DELETE SET NULL`, so a delete leaves every review
/// intact but pointing at nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProductDependents {
    /// Review threads linked to this product — the blocking dependency.
    pub reviews: u64,
    pub media: u64,
    pub materials: u64,
}

impl ProductDependents {
    /// Reviews are the only dependency that carries information a cascade would
    /// destroy; media and material links are safe to drop with the product.
    pub fn blocks_hard_delete(&self) -> bool {
        self.reviews > 0
    }
}

/// Partial update. Outer `None` = leave unchanged. For nullable columns,
/// `Some(None)` clears the value and `Some(Some(v))` sets it.
#[derive(Debug, Default, Clone)]
pub struct UpdateProduct {
    pub name: Option<String>,
    pub status: Option<ProductStatus>,
    pub brand_id: Option<Option<Uuid>>,
    pub category_id: Option<Option<Uuid>>,
    pub style: Option<Option<String>>,
    pub price_min: Option<Option<i32>>,
    pub price_max: Option<Option<i32>>,
    pub currency: Option<String>,
    pub dimensions: Option<Value>,
    pub origin: Option<Option<String>>,
    pub primary_image_key: Option<Option<String>>,
    pub description_md: Option<Option<String>>,
}

#[async_trait]
pub trait ProductRepository: Send + Sync {
    async fn create(&self, product: NewProduct) -> Result<Product, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Product>, AppError>;
    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Product>, AppError>;

    /// Returns a page of products (each with its aggregate rating) plus the
    /// total row count for the filter.
    async fn list(
        &self,
        filter: ProductListFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<ProductListItem>, u64), AppError>;

    async fn update(&self, id: Uuid, patch: UpdateProduct) -> Result<Product, AppError>;
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;

    /// Count the rows that reference this product, so a delete can be refused
    /// before it orphans review threads.
    async fn count_dependents(&self, product_id: Uuid) -> Result<ProductDependents, AppError>;

    /// Replace the product's material links with exactly `material_ids`.
    async fn set_materials(&self, product_id: Uuid, material_ids: &[Uuid])
        -> Result<(), AppError>;
    async fn list_material_ids(&self, product_id: Uuid) -> Result<Vec<Uuid>, AppError>;

    /// Map of `review_thread_id → published product's primary image key`, for
    /// threads whose linked product has a cover. Gives review list-cards a
    /// thumbnail fallback. Only published products are included.
    async fn primary_image_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, String>, AppError>;

    async fn add_media(&self, media: NewProductMedia) -> Result<ProductMedia, AppError>;
    async fn list_media(&self, product_id: Uuid) -> Result<Vec<ProductMedia>, AppError>;
    async fn find_media(&self, media_id: Uuid) -> Result<Option<ProductMedia>, AppError>;
    async fn delete_media(&self, media_id: Uuid) -> Result<(), AppError>;
}
