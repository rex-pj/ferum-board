use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

use crate::models::product::{NewProduct, Product, ProductStatus, ProductType};
use crate::models::product_category::{
    NewProductCategory, ProductCategory, UpdateProductCategory,
};
use crate::models::product_media::{NewProductMedia, ProductMedia};
use crate::AppError;

/// Ordering for catalog listing. On a review platform the default browse order
/// should still be "newest", but users need to sort by quality signals too.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ProductSort {
    #[default]
    Newest,
    /// Rating-quality order. NOT a raw average — a raw `avg_overall DESC` lets a
    /// single 5★ review outrank a 4.6★ backed by two hundred, which is the one
    /// ranking failure a review platform cannot afford. Implementations must
    /// apply the shrinkage in [`bayesian`].
    TopRated,
    MostReviewed,
}

/// Shrinkage constants for the `TopRated` ordering.
///
/// The score is a weighted average of the product's own mean and a global prior:
/// `(avg × n + PRIOR_MEAN × PRIOR_WEIGHT) / (n + PRIOR_WEIGHT)`. A product with
/// few reviews is pulled toward the prior and cannot top the list on one opinion;
/// as `n` grows the prior's influence decays and the product's own mean wins.
pub mod bayesian {
    /// The mean a product is assumed to have before any evidence. Deliberately
    /// mid-scale rather than optimistic: an unproven product should have to earn
    /// its way up, not start near the top.
    pub const PRIOR_MEAN: f64 = 3.5;
    /// How many "virtual" reviews the prior is worth. At `n = PRIOR_WEIGHT` the
    /// product's own mean and the prior carry equal weight.
    pub const PRIOR_WEIGHT: f64 = 10.0;
}

/// How a listing treats the forum category a product is filed under.
///
/// `Unassigned` is not a convenience — it is the surface that makes the
/// backfill gap visible. `products.category_id` is nullable and every product
/// created before categories were assignable has it NULL, so without a way to
/// list exactly those, a curator has no way to find what still needs filing and
/// the category filter stays quietly half-useless forever.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum CategoryFilter {
    #[default]
    Any,
    /// Only products with no category yet — a curator's work queue.
    Unassigned,
    /// The selected category plus its children (the hierarchy is two levels).
    /// An empty list means the same as `Any`; callers resolve the subtree.
    In(Vec<Uuid>),
}

/// Filters for catalog browsing. All fields are optional (AND-combined).
#[derive(Debug, Default, Clone)]
pub struct ProductListFilter {
    pub product_type: Option<ProductType>,
    pub status: Option<ProductStatus>,
    pub brand_id: Option<Uuid>,
    pub category: CategoryFilter,
    pub material_id: Option<Uuid>,
    pub query: Option<String>,
    pub sort: ProductSort,
    /// When set alongside a `status` filter, also include products created by
    /// this user regardless of status — so a contributor can still find their
    /// own pending submissions in the public list. `None` = strict status only.
    pub include_own: Option<Uuid>,
    /// Drop products with fewer than this many reviews. For surfaces that make a
    /// quality claim ("top rated") and must not make it on thin evidence — the
    /// shrinkage in [`bayesian`] fixes the *order*, this fixes *eligibility*.
    /// `None` = no floor, which is what plain browsing wants.
    pub min_review_count: Option<i32>,
}

/// A product enriched with its aggregate rating, for catalog cards. The average
/// is exact `numeric(4,2)` (no float jitter); `review_count` is 0 when unrated.
#[derive(Debug, Clone)]
pub struct ProductListItem {
    pub product: Product,
    pub review_count: i32,
    pub avg_overall: Option<Decimal>,
}

/// The product a review thread is about, as far as a list-card needs to know it.
/// `primary_image_key` is optional because a published product need not have a
/// cover yet; the slug and name always exist, so a card can always name and link
/// the subject of the review even with no image.
#[derive(Debug, Clone)]
pub struct ReviewedProduct {
    pub slug: String,
    pub name: String,
    pub primary_image_key: Option<String>,
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

/// Outcome of running the keyword matcher over unfiled products.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AutoAssignReport {
    /// Products that had no category and now have one.
    pub assigned: u64,
    /// Products still unfiled — no keyword matched their name. Left alone
    /// rather than dumped into a catch-all: a wrong category is worse than an
    /// honest blank, and this number is the size of the remaining manual queue.
    pub unmatched: u64,
}

#[async_trait]
pub trait ProductCategoryRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<ProductCategory>, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<ProductCategory>, AppError>;
    async fn create(&self, category: NewProductCategory) -> Result<ProductCategory, AppError>;
    async fn update(
        &self,
        id: Uuid,
        patch: UpdateProductCategory,
    ) -> Result<ProductCategory, AppError>;

    /// Deletes a category. Products filed under it fall back to unfiled
    /// (`ON DELETE SET NULL`) — a category is a label, and losing it must not
    /// take the product with it.
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;

    /// How many products are filed under each category id, plus how many are
    /// unfiled under the `None` key. Drives the admin list's counts and the
    /// size of the remaining backfill queue.
    async fn product_counts(&self) -> Result<std::collections::HashMap<Option<Uuid>, u64>, AppError>;

    /// Runs the keyword matcher over every product with no category and files
    /// the ones it can identify.
    ///
    /// Only touches `NULL` rows, so it is idempotent and can never overwrite a
    /// human's decision. `dry_run` reports what would happen without writing —
    /// an admin should be able to see the blast radius before taking it.
    async fn auto_assign_categories(&self, dry_run: bool) -> Result<AutoAssignReport, AppError>;
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

    /// Batch lookup with aggregate ratings attached, for hydrating search hits.
    /// Order is unspecified — the caller re-orders by relevance. Ids that do not
    /// resolve are simply absent, so a product deleted after indexing degrades
    /// to a missing card rather than an error.
    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<ProductListItem>, AppError>;

    async fn update(&self, id: Uuid, patch: UpdateProduct) -> Result<Product, AppError>;
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;

    /// Count the rows that reference this product, so a delete can be refused
    /// before it orphans review threads.
    async fn count_dependents(&self, product_id: Uuid) -> Result<ProductDependents, AppError>;

    /// Replace the product's material links with exactly `material_ids`.
    async fn set_materials(&self, product_id: Uuid, material_ids: &[Uuid])
        -> Result<(), AppError>;
    async fn list_material_ids(&self, product_id: Uuid) -> Result<Vec<Uuid>, AppError>;

    /// Map of `review_thread_id → the product it reviews`, for review list-cards:
    /// the cover is a thumbnail fallback, and the slug/name let a card say *what*
    /// was reviewed and link to it. Only published products are included.
    async fn reviewed_product_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, ReviewedProduct>, AppError>;

    async fn add_media(&self, media: NewProductMedia) -> Result<ProductMedia, AppError>;
    async fn list_media(&self, product_id: Uuid) -> Result<Vec<ProductMedia>, AppError>;
    async fn find_media(&self, media_id: Uuid) -> Result<Option<ProductMedia>, AppError>;
    async fn delete_media(&self, media_id: Uuid) -> Result<(), AppError>;
}
