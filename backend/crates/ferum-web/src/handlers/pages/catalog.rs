use std::collections::{BTreeMap, HashSet};

use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;
use uuid::Uuid;

use super::{active_theme, nav_categories_ctx, render_with_theme_in, user_ctx, PageError};
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

/// The catalogue taxonomy shaped for the templates: the filter `<select>` on
/// /catalog and the "Category" panel in the catalogue rail read the same rows,
/// so they are built once here rather than twice with two ideas of what a child
/// category looks like.
fn catalog_categories_ctx(
    categories: &[ferum_domain::models::product_category::ProductCategory],
) -> Vec<serde_json::Value> {
    categories
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id.to_string(),
                "name": c.name.clone(),
                "icon": c.icon.clone(),
                "is_child": c.parent_id.is_some(),
            })
        })
        .collect()
}

/// Context every catalogue surface (/catalog, /materials, /brands) needs for the
/// shared rail in `partials/catalog_sidebar.html`.
///
/// The rail is identical on all three tabs by design — switching tabs must not
/// change what sits beside the content — so its data is assembled in one place
/// rather than three times with three ideas of what belongs there. `categories`
/// is passed in because /catalog already holds them for its filter chips; a
/// second fetch here would be the same query twice per request.
///
/// The review read degrades to an empty panel rather than failing the page: the
/// rail supports the page, it is not the page.
async fn catalog_rail_ctx(
    state: &AppState,
    ctx: &mut Context,
    categories: &[ferum_domain::models::product_category::ProductCategory],
) {
    ctx.insert("catalog_categories", &catalog_categories_ctx(categories));
    ctx.insert("latest_reviews", &super::forum::latest_reviews_ctx(state).await);
}

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
    /// A category UUID, or the literal `none` for unfiled products. Not a
    /// `Uuid` because that third state is the one a curator needs — see
    /// `resolve_product_category_filter`.
    pub category_id: Option<String>,
    pub q: Option<String>,
    pub sort: Option<String>,
}

/// GET /catalog — public product browse (published only).
pub async fn catalog_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<CatalogQuery>,
) -> Result<impl IntoResponse, PageError> {
    let page = q.page.unwrap_or(1).max(1);

    let filter = ProductListFilter {
        product_type: q.product_type.as_deref().and_then(parse_product_type),
        status: Some(ProductStatus::Published),
        brand_id: q.brand_id,
        // Same resolver as search and the admin list, so "category" means the
        // same thing — including subtree expansion — wherever it is offered.
        category: crate::utils::resolve_product_category_filter(&state, q.category_id.as_deref())
            .await,
        material_id: q.material_id,
        query: q.q.clone(),
        sort: q.sort.as_deref().map(parse_product_sort).unwrap_or_default(),
        include_own: None,
        // No floor even under `?sort=top_rated`: the catalogue is a complete
        // index, and the Bayesian shrinkage already keeps thinly-reviewed
        // products from topping it. Hiding them here would make products the
        // reader navigated to unreachable.
        min_review_count: None,
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
    // Kept (not just inserted) so the chip builder below can name the active
    // brand/material without a second query.
    let materials_resp: Vec<MaterialResponse> =
        materials.into_iter().map(MaterialResponse::from).collect();
    let brands_resp: Vec<BrandResponse> = brands.into_iter().map(BrandResponse::from).collect();
    ctx.insert("materials", &materials_resp);
    ctx.insert("brands", &brands_resp);
    ctx.insert("total", &total);
    ctx.insert("page", &page);
    ctx.insert("has_prev", &(page > 1));
    ctx.insert("has_next", &(page * PER_PAGE < total));
    ctx.insert("active_type", &q.product_type);
    ctx.insert("active_material", &q.material_id.map(|u| u.to_string()));
    ctx.insert("active_brand", &q.brand_id.map(|u| u.to_string()));
    ctx.insert("active_sort", &q.sort);
    ctx.insert("active_category_id", &q.category_id);
    ctx.insert("search_query", &q.q);
    ctx.insert("catalog_tab", "products");

    // The catalogue taxonomy (Sofa, Ghế, Bàn …), not the forum's discussion
    // tree. Same list the search page offers, so the two agree. Kept raw for the
    // chip builder to name the active category.
    let categories_raw = state.product.list_categories().await.unwrap_or_default();
    catalog_rail_ctx(&state, &mut ctx, &categories_raw).await;

    // Active narrowing filters, each a removable chip — same shape and template
    // the search page uses, so the two read alike. Sort is ordering, not
    // filtering, so it is preserved across every removal rather than chipped.
    // `type` travels as a code resolved through the shared product_type_label
    // macro; the rest carry a resolved name.
    let mut active_filters: Vec<(&str, String, String, String)> = Vec::new();
    if let Some(t) = q.product_type.as_deref().filter(|s| !s.is_empty()) {
        active_filters.push(("type", t.to_string(), "code".into(), t.to_string()));
    }
    if let Some(bid) = q.brand_id {
        if let Some(b) = brands_resp.iter().find(|b| b.id == bid) {
            active_filters.push(("brand_id", bid.to_string(), "name".into(), b.name.clone()));
        }
    }
    if let Some(mid) = q.material_id {
        if let Some(m) = materials_resp.iter().find(|m| m.id == mid) {
            active_filters.push(("material_id", mid.to_string(), "name".into(), m.name.clone()));
        }
    }
    // Only a real category is chipped. The "none" (unfiled) state has no select
    // option and reaches here only via a crafted URL, so — like the search page —
    // it is left unchipped rather than shown as a filter that cannot be named.
    if let Some(cid) = q.category_id.as_deref().filter(|s| !s.is_empty() && *s != "none") {
        if let Some(c) = categories_raw.iter().find(|c| c.id.to_string() == cid) {
            active_filters.push(("category_id", cid.to_string(), "name".into(), c.name.clone()));
        }
    }

    let chips: Vec<serde_json::Value> = active_filters
        .iter()
        .map(|(key, _, kind, label)| {
            let mut parts: Vec<String> = Vec::new();
            if let Some(query) = q.q.as_deref().filter(|s| !s.trim().is_empty()) {
                parts.push(format!("q={}", urlencoding::encode(query)));
            }
            for (other_key, other_val, _, _) in &active_filters {
                if other_key != key {
                    parts.push(format!("{}={}", other_key, urlencoding::encode(other_val)));
                }
            }
            // Ordering survives every filter removal.
            if let Some(s) = q.sort.as_deref().filter(|s| !s.is_empty()) {
                parts.push(format!("sort={}", urlencoding::encode(s)));
            }
            serde_json::json!({
                "kind": kind,
                "label": label,
                "remove_url": format!("/catalog?{}", parts.join("&")),
            })
        })
        .collect();
    ctx.insert("filter_chips", &chips);

    // Every active filter, as a query-string fragment for the pager.
    //
    // The prev/next links previously carried nothing but `page`, so paging a
    // filtered catalogue silently dropped the filter and page 2 showed an
    // unrelated set. Assembled here rather than in the template so there is one
    // place that has to know the parameter names.
    let mut params: Vec<String> = Vec::new();
    if let Some(v) = q.q.as_deref().filter(|s| !s.trim().is_empty()) {
        params.push(format!("&q={}", urlencoding::encode(v)));
    }
    if let Some(v) = q.product_type.as_deref().filter(|s| !s.is_empty()) {
        params.push(format!("&type={}", urlencoding::encode(v)));
    }
    if let Some(v) = q.brand_id {
        params.push(format!("&brand_id={v}"));
    }
    if let Some(v) = q.material_id {
        params.push(format!("&material_id={v}"));
    }
    if let Some(v) = q.sort.as_deref().filter(|s| !s.is_empty()) {
        params.push(format!("&sort={}", urlencoding::encode(v)));
    }

    // Everything active EXCEPT the category, for the rail's category rows. They
    // replace the category rather than add to it, so re-emitting the current one
    // would produce a link that cannot change anything — and dropping the rest
    // would silently clear the reader's brand/material/query, which is the bug
    // the pager had before `filter_params` existed.
    let category_link_params = params.join("");
    let catalog_all_url = if category_link_params.is_empty() {
        "/catalog".to_string()
    } else {
        format!("/catalog?{}", category_link_params.trim_start_matches('&'))
    };
    ctx.insert("category_link_params", &category_link_params);
    ctx.insert("catalog_all_url", &catalog_all_url);

    if let Some(v) = q.category_id.as_deref().filter(|s| !s.is_empty()) {
        params.push(format!("&category_id={}", urlencoding::encode(v)));
    }
    ctx.insert("filter_params", &params.join(""));
    ctx.insert(
        "can_submit_product",
        &auth_user.as_ref().map_or(false, |u| PermissionChecker::can_submit_products(u).is_ok()),
    );

    render_with_theme_in(&state, &req_locale, &active, "catalog/index.html", &ctx).await
}

/// GET /materials — public reference list of materials, grouped by category,
/// each entry linking into the catalog filtered by that material.
pub async fn materials_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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
    let categories = state.product.list_categories().await.unwrap_or_default();
    catalog_rail_ctx(&state, &mut ctx, &categories).await;

    render_with_theme_in(&state, &req_locale, &active, "catalog/materials.html", &ctx).await
}

/// GET /brands — public brand directory; each entry links into the catalog
/// filtered by that brand.
pub async fn brands_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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
    let categories = state.product.list_categories().await.unwrap_or_default();
    catalog_rail_ctx(&state, &mut ctx, &categories).await;

    render_with_theme_in(&state, &req_locale, &active, "catalog/brands.html", &ctx).await
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
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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

    // Captured before `product` is consumed below; `ProductResponse` does not
    // carry the submitter, and it should not — it is serialised to the public API.
    let product_created_by = product.created_by_id;
    let product_is_draft = product.status == ferum_domain::models::product::ProductStatus::Draft;

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
    // Curators get an edit affordance on the product itself, rather than having
    // to find it again in the admin catalogue.
    let can_manage =
        auth_user.as_ref().map_or(false, |u| PermissionChecker::can_manage_products(u).is_ok());
    ctx.insert("can_manage_product", &can_manage);
    // Whoever submitted this product, excluding curators (who already have the
    // controls and need no notice). Drives the explanatory note: once an entry is
    // approved it stops being theirs, and showing nothing at all reads as a bug.
    let is_own_submission = !can_manage
        && match (&auth_user, product_created_by) {
            (Some(u), Some(creator)) => u.id == creator,
            _ => false,
        };
    ctx.insert("is_own_submission", &is_own_submission);
    // Mirrors `ProductUseCase::authorize_product_edit`. Kept in step with it
    // deliberately: this only decides whether to draw the controls, and drawing
    // them when the use case would refuse is how you build a button that 403s.
    ctx.insert(
        "can_edit_product",
        &(can_manage || (is_own_submission && product_is_draft)),
    );

    render_with_theme_in(&state, &req_locale, &active, "catalog/product.html", &ctx).await
}
