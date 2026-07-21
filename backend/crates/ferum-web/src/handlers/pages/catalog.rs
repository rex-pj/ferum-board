use std::collections::{BTreeMap, HashSet};

use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;
use uuid::Uuid;

use super::{active_theme, nav_categories_ctx, render_with_theme, user_ctx, PageError};
use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use ferum_application::constants::MIN_REVIEWS_FOR_RATING_BREAKDOWN;
use ferum_application::permission::PermissionChecker;
use crate::view_models::product::{
    parse_product_sort, parse_product_type, BrandResponse, MaterialResponse, ProductDetailResponse,
    ProductResponse, ProductReviewCtx,
};
use ferum_domain::models::product::ProductStatus;
use ferum_domain::repositories::product_repository::ProductListFilter;

const PER_PAGE: u64 = 24;

#[derive(Deserialize)]
pub struct CatalogQuery {
    pub page: Option<u64>,
    #[serde(rename = "type")]
    pub product_type: Option<String>,
    // Empty `material_id=` / `brand_id=` (the "All" <select> options) must
    // deserialize to None rather than fail — see empty_string_as_none_uuid.
    #[serde(default, deserialize_with = "crate::view_models::product::empty_string_as_none_uuid")]
    pub material_id: Option<Uuid>,
    #[serde(default, deserialize_with = "crate::view_models::product::empty_string_as_none_uuid")]
    pub brand_id: Option<Uuid>,
    pub q: Option<String>,
    pub sort: Option<String>,
}

/// GET /catalog — public product browse (published only).
pub async fn catalog_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<CatalogQuery>,
) -> Result<impl IntoResponse, PageError> {
    let page = q.page.unwrap_or(1).max(1);

    let filter = ProductListFilter {
        product_type: q.product_type.as_deref().and_then(parse_product_type),
        status: Some(ProductStatus::Published),
        brand_id: q.brand_id,
        category_id: None,
        material_id: q.material_id,
        query: q.q.clone(),
        sort: q.sort.as_deref().map(parse_product_sort).unwrap_or_default(),
        include_own: None,
    };
    let (products, total) = state.product.list(filter, page, PER_PAGE).await?;
    let materials = state.product.list_materials(None).await?;
    let brands = state.product.list_brands().await.unwrap_or_default();

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert(
        "products",
        &products.into_iter().map(ProductResponse::from).collect::<Vec<_>>(),
    );
    ctx.insert(
        "materials",
        &materials.into_iter().map(MaterialResponse::from).collect::<Vec<_>>(),
    );
    ctx.insert(
        "brands",
        &brands.into_iter().map(BrandResponse::from).collect::<Vec<_>>(),
    );
    ctx.insert("total", &total);
    ctx.insert("page", &page);
    ctx.insert("has_prev", &(page > 1));
    ctx.insert("has_next", &(page * PER_PAGE < total));
    ctx.insert("active_type", &q.product_type);
    ctx.insert("active_material", &q.material_id.map(|u| u.to_string()));
    ctx.insert("active_brand", &q.brand_id.map(|u| u.to_string()));
    ctx.insert("active_sort", &q.sort);
    ctx.insert("search_query", &q.q);
    ctx.insert("catalog_tab", "products");
    ctx.insert(
        "can_submit_product",
        &auth_user.as_ref().map_or(false, |u| PermissionChecker::can_submit_products(u).is_ok()),
    );

    render_with_theme(&state, &active, "catalog/index.html", &ctx).await
}

/// GET /materials — public reference list of materials, grouped by category,
/// each entry linking into the catalog filtered by that material.
pub async fn materials_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    let materials = state.product.list_materials(None).await?;

    // Group by material category (BTreeMap → stable alphabetical section order).
    let mut grouped: BTreeMap<String, Vec<MaterialResponse>> = BTreeMap::new();
    for m in materials {
        grouped
            .entry(m.category.clone())
            .or_default()
            .push(MaterialResponse::from(m));
    }
    let material_groups: Vec<serde_json::Value> = grouped
        .into_iter()
        .map(|(category, items)| serde_json::json!({ "category": category, "items": items }))
        .collect();

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("material_groups", &material_groups);
    ctx.insert("catalog_tab", "materials");
    ctx.insert(
        "can_submit_product",
        &auth_user.as_ref().map_or(false, |u| PermissionChecker::can_submit_products(u).is_ok()),
    );

    render_with_theme(&state, &active, "catalog/materials.html", &ctx).await
}

/// GET /brands — public brand directory; each entry links into the catalog
/// filtered by that brand.
pub async fn brands_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    let brands: Vec<BrandResponse> = state
        .product
        .list_brands()
        .await?
        .into_iter()
        .map(BrandResponse::from)
        .collect();

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("brands", &brands);
    ctx.insert("catalog_tab", "brands");
    ctx.insert(
        "can_submit_product",
        &auth_user.as_ref().map_or(false, |u| PermissionChecker::can_submit_products(u).is_ok()),
    );

    render_with_theme(&state, &active, "catalog/brands.html", &ctx).await
}

/// Sort/filter controls for the review list on a product page.
#[derive(Deserialize)]
pub struct ReviewSortQuery {
    pub rsort: Option<String>,
    pub verified: Option<String>,
}

/// GET /catalog/:slug — public product detail with materials, media, and rating stats.
pub async fn catalog_detail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Query(rq): Query<ReviewSortQuery>,
) -> Result<impl IntoResponse, PageError> {
    let product = state
        .product
        .get_by_slug(&slug)
        .await
        .map_err(|_| PageError::NotFound)?;
    if product.status != ProductStatus::Published {
        return Err(PageError::NotFound);
    }
    let product_id = product.id;

    let material_ids: HashSet<Uuid> = state
        .product
        .list_material_ids(product_id)
        .await?
        .into_iter()
        .collect();
    let materials: Vec<MaterialResponse> = state
        .product
        .list_materials(None)
        .await?
        .into_iter()
        .filter(|m| material_ids.contains(&m.id))
        .map(MaterialResponse::from)
        .collect();
    let media = state
        .product
        .list_media(product_id)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    let rating_stats = state.review.product_stats(product_id).await?.map(Into::into);

    // Review threads for this product + their ratings (2 queries, no N+1).
    let review_threads = state
        .thread
        .list_reviews_for_product(product_id, 50)
        .await
        .unwrap_or_default();
    let thread_ids: Vec<Uuid> = review_threads.iter().map(|t| t.id).collect();
    let ratings = state
        .review
        .ratings_for_threads(&thread_ids)
        .await
        .unwrap_or_default();
    let mut reviews: Vec<ProductReviewCtx> = review_threads
        .iter()
        .map(|t| {
            let r = ratings.get(&t.id);
            let author = t.author_username.clone().unwrap_or_default();
            ProductReviewCtx {
                slug: t.slug.clone(),
                title: t.title.clone(),
                author_display_name: t
                    .author_display_name
                    .clone()
                    .unwrap_or_else(|| author.clone()),
                author_username: author,
                author_avatar_url: t.author_avatar_url.clone(),
                created_at: t.created_at.to_rfc3339(),
                reply_count: t.reply_count,
                excerpt: t.excerpt.clone(),
                overall: r.map(|x| x.overall).unwrap_or(0),
                durability: r.and_then(|x| x.durability),
                materials: r.and_then(|x| x.materials),
                comfort: r.and_then(|x| x.comfort),
                aesthetics: r.and_then(|x| x.aesthetics),
                value_for_money: r.and_then(|x| x.value_for_money),
                verified_purchase: r.map(|x| x.verified_purchase).unwrap_or(false),
            }
        })
        .collect();

    // Filter + sort the loaded review set (capped at 50). created_at is RFC3339,
    // which sorts correctly as a string.
    let verified_only = rq.verified.as_deref() == Some("1");
    if verified_only {
        reviews.retain(|r| r.verified_purchase);
    }
    let rsort = rq.rsort.as_deref().unwrap_or("newest");
    match rsort {
        "highest" => reviews.sort_by(|a, b| {
            b.overall.cmp(&a.overall).then_with(|| b.created_at.cmp(&a.created_at))
        }),
        "lowest" => reviews.sort_by(|a, b| {
            a.overall.cmp(&b.overall).then_with(|| b.created_at.cmp(&a.created_at))
        }),
        _ => reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
    }

    // Star-distribution histogram (index 0 = 1★ … 4 = 5★).
    let rating_distribution = state
        .review
        .rating_distribution(product_id)
        .await
        .unwrap_or([0; 5]);

    // A user may review a product once. If this one already has, the page links
    // to that review instead of offering a Write button that would 409.
    let my_review_slug: Option<String> = match auth_user.as_ref() {
        Some(u) => state
            .thread
            .find_review_by_author(product_id, u.id)
            .await
            .unwrap_or(None)
            .map(|t| t.slug),
        None => None,
    };

    let brand = match product.brand_id {
        Some(bid) => state.product.find_brand(bid).await?.map(BrandResponse::from),
        None => None,
    };

    let detail = ProductDetailResponse {
        product: product.into(),
        brand,
        materials,
        media,
        rating_stats,
    };

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("product", &detail);
    ctx.insert("reviews", &reviews);
    // Histogram rows 5★ → 1★, each with its share of the total (computed here so
    // the template does no float math).
    let dist_total: i32 = rating_distribution.iter().sum();
    let dist_rows: Vec<serde_json::Value> = (1..=5)
        .rev()
        .map(|star| {
            let count = rating_distribution[star - 1];
            let pct = if dist_total > 0 {
                (count as f64 / dist_total as f64 * 100.0).round() as i32
            } else {
                0
            };
            serde_json::json!({ "star": star, "count": count, "pct": pct })
        })
        .collect();
    ctx.insert("rating_distribution", &dist_rows);
    // Sample size below which the template collapses the histogram and the
    // per-dimension bars into a compact chip list.
    ctx.insert("stats_min_reviews", &MIN_REVIEWS_FOR_RATING_BREAKDOWN);
    ctx.insert("my_review_slug", &my_review_slug);
    ctx.insert("active_rsort", &rsort);
    ctx.insert("verified_only", &verified_only);

    render_with_theme(&state, &active, "catalog/product.html", &ctx).await
}
