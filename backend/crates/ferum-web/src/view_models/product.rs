use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::brand::Brand;
use ferum_domain::models::material::Material;
use ferum_domain::models::product::{Product, ProductStatus, ProductType};
use ferum_domain::models::product_category::ProductCategory;
use ferum_domain::models::product_media::ProductMedia;
use ferum_domain::models::product_rating_stats::ProductRatingStats;
use ferum_domain::models::review_rating::ReviewRating;
use ferum_domain::repositories::product_repository::{ProductListItem, ProductSort};

// ─── enum parsing ──────────────────────────────────────────────────────────────

pub fn parse_product_type(s: &str) -> Option<ProductType> {
    match s {
        "furniture" => Some(ProductType::Furniture),
        "material" => Some(ProductType::Material),
        "room" => Some(ProductType::Room),
        _ => None,
    }
}

pub fn parse_product_status(s: &str) -> Option<ProductStatus> {
    match s {
        "draft" => Some(ProductStatus::Draft),
        "published" => Some(ProductStatus::Published),
        "archived" => Some(ProductStatus::Archived),
        _ => None,
    }
}

/// Deserialize an optional UUID query param, treating an empty/blank string as
/// `None` (HTML `<select>` "All" options submit `field=`) and an unparseable
/// value as `None` too (a broken filter param shouldn't 500 the page).
///
/// The second half of that is an SSR-only concession. For JSON APIs use
/// [`crate::view_models::blank_as_none_uuid`], which still rejects garbage
/// rather than silently returning unfiltered results.
pub fn empty_string_as_none_uuid<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.and_then(|s| {
        let s = s.trim();
        if s.is_empty() {
            None
        } else {
            Uuid::parse_str(s).ok()
        }
    }))
}

/// Maps the `?sort=` query value to an ordering; unknown/absent → Newest.
pub fn parse_product_sort(s: &str) -> ProductSort {
    match s {
        "top_rated" => ProductSort::TopRated,
        "most_reviewed" => ProductSort::MostReviewed,
        _ => ProductSort::Newest,
    }
}

fn product_type_str(t: ProductType) -> &'static str {
    match t {
        ProductType::Furniture => "furniture",
        ProductType::Material => "material",
        ProductType::Room => "room",
    }
}

fn product_status_str(s: ProductStatus) -> &'static str {
    match s {
        ProductStatus::Draft => "draft",
        ProductStatus::Published => "published",
        ProductStatus::Archived => "archived",
    }
}

// ─── Requests ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct ProductListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    #[serde(rename = "type")]
    pub product_type: Option<String>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none_uuid")]
    pub brand_id: Option<Uuid>,
    /// A category UUID, or the literal `none` for products not filed under any
    /// category. Typed as a string because `Option<Uuid>` cannot carry that
    /// third state — see `resolve_product_category_filter`.
    pub category_id: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none_uuid")]
    pub material_id: Option<Uuid>,
    pub q: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateProductRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(
        length(min = 1, max = 200),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: String,
    /// "furniture" | "material" | "room"
    #[serde(default = "default_product_type")]
    pub product_type: String,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub style: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: Option<String>,
    #[serde(default)]
    pub dimensions: Option<Value>,
    pub origin: Option<String>,
    pub description_md: Option<String>,
    #[serde(default)]
    pub material_ids: Vec<Uuid>,
}

fn default_product_type() -> String {
    "furniture".to_string()
}

/// Member-facing product submission. No `slug` (derived server-side) and no
/// `status` (always created as draft, pending admin approval).
#[derive(Debug, Deserialize, Validate)]
pub struct SubmitProductRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[serde(default = "default_product_type")]
    pub product_type: String,
    pub brand_id: Option<Uuid>,
    pub style: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub origin: Option<String>,
    pub description_md: Option<String>,
    #[serde(default)]
    pub material_ids: Vec<Uuid>,
}

/// Partial update. `Option<Option<T>>` on nullable fields: outer None = leave,
/// Some(None) = clear, Some(Some(v)) = set.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub status: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct SetMaterialsRequest {
    #[serde(default)]
    pub material_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateBrandRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub country: Option<String>,
    pub is_verified: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMaterialRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[validate(length(min = 1, max = 40))]
    pub category: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateBrandRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(
        length(min = 1, max = 200),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub country: Option<String>,
    /// Present so the admin "Verified" checkbox survives creation; without it the
    /// brand had to be created and then edited to take effect.
    #[serde(default)]
    pub is_verified: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateMaterialRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[validate(
        length(min = 1, max = 120),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: String,
    #[validate(length(min = 1, max = 40))]
    pub category: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitRatingRequest {
    pub overall: i16,
    pub durability: Option<i16>,
    pub materials: Option<i16>,
    pub comfort: Option<i16>,
    pub aesthetics: Option<i16>,
    pub value_for_money: Option<i16>,
    #[serde(default)]
    pub verified_purchase: bool,
}

// ─── Responses ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MaterialResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub category: String,
    pub description: Option<String>,
}

// ─── Product categories ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct CreateProductCategoryRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[validate(
        length(min = 1, max = 120),
        custom(function = "crate::view_models::validators::slug_format")
    )]
    pub slug: String,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub position: i32,
    pub icon: Option<String>,
    /// Words that identify this category inside a product name. Lowercased and
    /// de-duplicated by the use case, since the matcher compares lowercased.
    #[serde(default)]
    pub match_keywords: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateProductCategoryRequest {
    pub name: Option<String>,
    pub parent_id: Option<Option<Uuid>>,
    pub position: Option<i32>,
    pub icon: Option<Option<String>>,
    pub match_keywords: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct ProductCategoryResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub icon: Option<String>,
    pub match_keywords: Vec<String>,
    /// How many products are filed here. Only populated on the admin listing —
    /// public callers get 0, since the count is a curation signal, not catalogue
    /// data.
    pub product_count: u64,
}

impl From<ProductCategory> for ProductCategoryResponse {
    fn from(c: ProductCategory) -> Self {
        Self {
            id: c.id,
            slug: c.slug,
            name: c.name,
            parent_id: c.parent_id,
            position: c.position,
            icon: c.icon,
            match_keywords: c.match_keywords,
            product_count: 0,
        }
    }
}

/// What the auto-assign matcher did, or would do on a dry run.
#[derive(Serialize)]
pub struct AutoAssignResponse {
    pub assigned: u64,
    /// Products no keyword could identify. This is the size of the manual queue
    /// that remains, which is the number an admin actually plans around.
    pub unmatched: u64,
    pub dry_run: bool,
}

impl From<Material> for MaterialResponse {
    fn from(m: Material) -> Self {
        Self {
            id: m.id,
            slug: m.slug,
            name: m.name,
            category: m.category,
            description: m.description,
        }
    }
}

#[derive(Serialize)]
pub struct BrandResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub website: Option<String>,
    pub country: Option<String>,
    pub is_verified: bool,
    pub tier: String,
}

impl From<Brand> for BrandResponse {
    fn from(b: Brand) -> Self {
        Self {
            id: b.id,
            slug: b.slug,
            name: b.name,
            description: b.description,
            logo_url: b.logo_url,
            website: b.website,
            country: b.country,
            is_verified: b.is_verified,
            tier: b.tier,
        }
    }
}

#[derive(Serialize)]
pub struct ProductResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub product_type: String,
    pub status: String,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub style: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: String,
    pub dimensions: Value,
    pub origin: Option<String>,
    pub primary_image_key: Option<String>,
    pub description_md: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Aggregate rating for catalog cards. `review_count` is 0 and `avg_overall`
    /// is null on responses built from a bare `Product` (e.g. product detail,
    /// which carries full `rating_stats` separately).
    pub review_count: i32,
    pub avg_overall: Option<f64>,
}

impl From<Product> for ProductResponse {
    fn from(p: Product) -> Self {
        Self {
            id: p.id,
            slug: p.slug,
            name: p.name,
            product_type: product_type_str(p.product_type).to_string(),
            status: product_status_str(p.status).to_string(),
            brand_id: p.brand_id,
            category_id: p.category_id,
            style: p.style,
            price_min: p.price_min,
            price_max: p.price_max,
            currency: p.currency,
            dimensions: p.dimensions,
            origin: p.origin,
            primary_image_key: p.primary_image_key,
            description_md: p.description_md,
            created_at: p.created_at,
            review_count: 0,
            avg_overall: None,
        }
    }
}

impl From<ProductListItem> for ProductResponse {
    fn from(item: ProductListItem) -> Self {
        let review_count = item.review_count;
        let avg_overall = item.avg_overall.and_then(|v| v.to_f64());
        Self {
            review_count,
            avg_overall,
            ..Self::from(item.product)
        }
    }
}

#[derive(Serialize)]
pub struct ProductMediaResponse {
    pub id: Uuid,
    pub storage_key: String,
    pub kind: String,
    pub caption: Option<String>,
    pub position: i32,
}

impl From<ProductMedia> for ProductMediaResponse {
    fn from(m: ProductMedia) -> Self {
        Self {
            id: m.id,
            storage_key: m.storage_key,
            kind: m.kind,
            caption: m.caption,
            position: m.position,
        }
    }
}

/// Preflight for the admin delete dialog: what this product would take with it,
/// and whether a permanent delete is allowed at all.
#[derive(Serialize)]
pub struct ProductDependentsResponse {
    pub reviews: u64,
    pub media: u64,
    pub materials: u64,
    pub can_hard_delete: bool,
}

#[derive(Serialize)]
pub struct RatingStatsResponse {
    pub review_count: i32,
    pub avg_overall: Option<f64>,
    pub avg_durability: Option<f64>,
    pub avg_materials: Option<f64>,
    pub avg_comfort: Option<f64>,
    pub avg_aesthetics: Option<f64>,
    pub avg_value_for_money: Option<f64>,
}

impl From<ProductRatingStats> for RatingStatsResponse {
    fn from(s: ProductRatingStats) -> Self {
        let f = |d: Option<rust_decimal::Decimal>| d.and_then(|v| v.to_f64());
        Self {
            review_count: s.review_count,
            avg_overall: f(s.avg_overall),
            avg_durability: f(s.avg_durability),
            avg_materials: f(s.avg_materials),
            avg_comfort: f(s.avg_comfort),
            avg_aesthetics: f(s.avg_aesthetics),
            avg_value_for_money: f(s.avg_value_for_money),
        }
    }
}

#[derive(Serialize)]
pub struct ReviewRatingResponse {
    pub thread_id: Uuid,
    pub overall: i16,
    pub durability: Option<i16>,
    pub materials: Option<i16>,
    pub comfort: Option<i16>,
    pub aesthetics: Option<i16>,
    pub value_for_money: Option<i16>,
    pub verified_purchase: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<ReviewRating> for ReviewRatingResponse {
    fn from(r: ReviewRating) -> Self {
        Self {
            thread_id: r.thread_id,
            overall: r.overall,
            durability: r.durability,
            materials: r.materials,
            comfort: r.comfort,
            aesthetics: r.aesthetics,
            value_for_money: r.value_for_money,
            verified_purchase: r.verified_purchase,
            updated_at: r.updated_at,
        }
    }
}

/// One review row on the product detail page (thread + its author + score).
#[derive(Serialize)]
pub struct ProductReviewCtx {
    pub slug: String,
    pub title: String,
    pub author_username: String,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    pub created_at: String,
    pub reply_count: i32,
    pub excerpt: Option<String>,
    pub overall: i16,
    pub durability: Option<i16>,
    pub materials: Option<i16>,
    pub comfort: Option<i16>,
    pub aesthetics: Option<i16>,
    pub value_for_money: Option<i16>,
    pub verified_purchase: bool,
}

/// Full product view: the product plus its brand, materials, media, and stats.
#[derive(Serialize)]
pub struct ProductDetailResponse {
    #[serde(flatten)]
    pub product: ProductResponse,
    pub brand: Option<BrandResponse>,
    pub materials: Vec<MaterialResponse>,
    pub media: Vec<ProductMediaResponse>,
    pub rating_stats: Option<RatingStatsResponse>,
}
