use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_application::usecases::search_usecase::{
    HydratedProductHit, HydratedSearchHit, SearchOutcome,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    /// Tolerant of `?category_id=` — an HTML `<select>`'s "All" option submits the
    /// key with an empty value, and a bare `Option<Uuid>` fails the whole extractor.
    /// A *malformed* id is still an error: this is an API, and quietly dropping a
    /// filter would return results the caller never asked for.
    #[serde(default, deserialize_with = "crate::view_models::blank_as_none_uuid")]
    pub category_id: Option<Uuid>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    /// `all` (default) | `products` | `threads`. Matches the `?tab=` on the SSR
    /// results page, so a client and the page ask the same question.
    pub tab: Option<String>,

    /// Product facets, named as on the results page and the catalogue.
    #[serde(rename = "type")]
    pub product_type: Option<String>,
    #[serde(default, deserialize_with = "crate::view_models::blank_as_none_uuid")]
    pub brand_id: Option<Uuid>,
    #[serde(default, deserialize_with = "crate::view_models::blank_as_none_uuid")]
    pub material_id: Option<Uuid>,
    pub psort: Option<String>,
    /// Discussion ordering, matching `?tsort=` on the SSR page.
    pub tsort: Option<String>,
    /// Catalogue category. Named `pcat` because `category_id` on this endpoint
    /// is the *forum* category — two different taxonomies, one request.
    pub pcat: Option<String>,
}

#[derive(Serialize)]
pub struct ThreadHitResponse {
    pub thread_id: Uuid,
    pub thread_slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub author_username: Option<String>,
    pub reply_count: i32,
}

impl From<&HydratedSearchHit> for ThreadHitResponse {
    fn from(h: &HydratedSearchHit) -> Self {
        Self {
            thread_id: h.thread_id,
            thread_slug: h.thread_slug.clone(),
            title: h.title.clone(),
            excerpt: h.excerpt.clone(),
            category_slug: h.category_slug.clone(),
            category_name: h.category_name.clone(),
            author_username: h.author_username.clone(),
            reply_count: h.reply_count,
        }
    }
}

#[derive(Serialize)]
pub struct ProductHitResponse {
    pub product_id: Uuid,
    pub slug: String,
    pub name: String,
    pub excerpt: Option<String>,
    pub product_type: String,
    pub primary_image_key: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: String,
    pub review_count: i32,
    pub avg_overall: Option<f64>,
}

impl From<&HydratedProductHit> for ProductHitResponse {
    fn from(p: &HydratedProductHit) -> Self {
        Self {
            product_id: p.product_id,
            slug: p.slug.clone(),
            name: p.name.clone(),
            excerpt: p.excerpt.clone(),
            product_type: p.product_type.clone(),
            primary_image_key: p.primary_image_key.clone(),
            price_min: p.price_min,
            price_max: p.price_max,
            currency: p.currency.clone(),
            review_count: p.review_count,
            avg_overall: p.avg_overall,
        }
    }
}

/// Grouped rather than one flat list: the two kinds are ranked on incomparable
/// scales, and a client rendering them as sections needs them apart anyway.
/// Totals cover the full match set, so a caller can label its own tabs without
/// a second request.
#[derive(Serialize)]
pub struct SearchResponse {
    pub threads: Vec<ThreadHitResponse>,
    pub products: Vec<ProductHitResponse>,
    pub thread_total: u64,
    pub product_total: u64,
}

impl From<SearchOutcome> for SearchResponse {
    fn from(o: SearchOutcome) -> Self {
        Self {
            threads: o.threads.iter().map(ThreadHitResponse::from).collect(),
            products: o.products.iter().map(ProductHitResponse::from).collect(),
            thread_total: o.thread_total,
            product_total: o.product_total,
        }
    }
}
