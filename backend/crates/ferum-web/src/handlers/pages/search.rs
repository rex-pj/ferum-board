use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{
    CategoryCtx, PaginationCtx, SearchHitCtx, SearchProductCtx,
};
use crate::view_models::product::{parse_product_type, BrandResponse, MaterialResponse};
use ferum_application::permission::PermissionChecker;
use ferum_application::ports::{ProductFacets, ProductSearchSort, ThreadSearchSort};
use ferum_application::usecases::search_usecase::{SearchOutcome, SearchRequest, SearchScope};

use super::{active_theme, nav_categories_ctx, post_policy_str, view_policy_str, render_with_theme_in, user_ctx, PageError};

const PER_PAGE: u64 = 20;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub page: Option<u64>,
    #[serde(default, deserialize_with = "crate::view_models::blank_as_none_uuid")]
    pub category_id: Option<uuid::Uuid>,
    /// Which result section is in focus: `all` (default), `products`, `threads`.
    pub tab: Option<String>,

    // ── Product facets ──────────────────────────────────────────────────────
    // Named to match the catalogue's own query params so a filter carries over
    // when a reader moves between /catalog and /search.
    #[serde(rename = "type")]
    pub product_type: Option<String>,
    #[serde(default, deserialize_with = "crate::view_models::product::empty_string_as_none_uuid")]
    pub brand_id: Option<uuid::Uuid>,
    #[serde(default, deserialize_with = "crate::view_models::product::empty_string_as_none_uuid")]
    pub material_id: Option<uuid::Uuid>,
    /// Product ordering. Distinct from the catalogue's `sort` because the
    /// default differs (relevance here, newest there) and because the two
    /// controls appear on the same page in the thread/product split.
    pub psort: Option<String>,
    /// Discussion ordering. Named apart from `psort` because thread and product
    /// results have different sort options and appear as separate tabs; relevance
    /// by default, matching a search page's one unarguable ordering.
    pub tsort: Option<String>,
    /// Catalogue category (Sofa, Ghế, Bàn …). Named `pcat` rather than
    /// `category_id` because that parameter is already taken by the *forum*
    /// category on this same page, and the two are different taxonomies.
    pub pcat: Option<String>,
}

#[tracing::instrument(skip(state, auth_user), fields(q = q.q.as_deref(), page = q.page))]
pub async fn search(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, PageError> {
    let active = active_theme(&state).await;
    let query = q.q.as_deref().unwrap_or_default().trim().to_string();
    let page = q.page.unwrap_or(1).max(1);
    let scope = q.tab.as_deref().map(SearchScope::parse).unwrap_or_default();

    // Two taxonomies, deliberately not merged.
    //
    // `category_id` is a *forum* category and narrows discussions only —
    // "Off-Topic" and "Programming" say nothing about a sofa. `pcat` is the
    // *catalogue* category (Sofa, Ghế, Bàn) and narrows products only. Each is
    // dropped on the tab where it does nothing rather than left in the URL to
    // quietly shrink a badge count the reader cannot account for.
    let category_id = match scope {
        SearchScope::Products => None,
        _ => q.category_id,
    };
    let product_category_ids = match scope {
        SearchScope::Threads => Vec::new(),
        _ => crate::utils::resolve_product_category_ids(&state, q.pcat.as_deref()).await,
    };
    let facets = match scope {
        SearchScope::Threads => ProductFacets::default(),
        _ => ProductFacets {
            product_type: q.product_type.as_deref().and_then(parse_product_type),
            brand_id: q.brand_id,
            material_id: q.material_id,
            sort: q.psort.as_deref().map(ProductSearchSort::parse).unwrap_or_default(),
        },
    };
    // Parsed on every tab, not just Discussions, so the choice survives a trip
    // through the Products tab. It only *orders* thread hits (a no-op for the
    // product-side badge count), so carrying it everywhere costs nothing.
    let thread_sort = q.tsort.as_deref().map(ThreadSearchSort::parse).unwrap_or_default();

    // Three distinct outcomes the template must be able to tell apart:
    // no query yet, a query with zero matches, and a query the search
    // backend failed to answer. Collapsing the last two into "no results"
    // tells the user their content doesn't exist when the engine is down.
    let mut search_error = false;
    let outcome = if query.is_empty() {
        SearchOutcome::default()
    } else {
        match state
            .search
            .search_all(
                auth_user.as_ref(),
                SearchRequest {
                    q: query.clone(),
                    scope,
                    category_id,
                    product_category_ids: product_category_ids.clone(),
                    facets: facets.clone(),
                    thread_sort,
                    page,
                    per_page: PER_PAGE,
                },
            )
            .await
        {
            Ok(o) => o,
            Err(e) => {
                tracing::error!(error = %e, q = %query, "search_page_failed");
                search_error = true;
                SearchOutcome::default()
            }
        }
    };

    let results: Vec<SearchHitCtx> = outcome
        .threads
        .iter()
        .map(|h| SearchHitCtx {
            thread_slug: h.thread_slug.clone(),
            thread_title: h.title.clone(),
            excerpt: h.excerpt.clone(),
            category_slug: h.category_slug.clone(),
            category_name: h.category_name.clone(),
            author_username: h.author_username.clone(),
            author_display_name: h.author_display_name.clone(),
            created_at: h.created_at.map(|d| d.to_rfc3339()),
            reply_count: h.reply_count,
        })
        .collect();

    let products: Vec<SearchProductCtx> = outcome
        .products
        .iter()
        .map(|p| SearchProductCtx {
            slug: p.slug.clone(),
            name: p.name.clone(),
            excerpt: p.excerpt.clone(),
            product_type: p.product_type.clone(),
            style: p.style.clone(),
            primary_image_key: p.primary_image_key.clone(),
            price_min: p.price_min,
            price_max: p.price_max,
            currency: p.currency.clone(),
            review_count: p.review_count,
            avg_overall: p.avg_overall,
        })
        .collect();

    let all_categories: Vec<CategoryCtx> = state
        .category
        .list_visible(auth_user.as_ref())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| CategoryCtx {
            id: c.id.to_string(),
            parent_id: c.parent_id.map(|id| id.to_string()),
            slug: c.slug.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            color: c.color.clone(),
            thread_count: 0,
            view_policy: view_policy_str(&c.view_policy),
            post_policy: post_policy_str(&c.post_policy),
            can_post: false,
        })
        .collect();

    // Pagination applies to whichever kind the active tab paginates. On `all`
    // that is discussions; the product row there is a fixed-size preview.
    let paged_total = match scope {
        SearchScope::Products => outcome.product_total,
        _ => outcome.thread_total,
    };

    // Every active control must ride along on the pagination links, or page 2
    // silently drops the reader's filters.
    let mut search_extra: Vec<String> = Vec::new();
    if !query.is_empty() {
        search_extra.push(format!("&q={}", urlencoding::encode(&query)));
    }
    if let Some(cid) = category_id {
        search_extra.push(format!("&category_id={cid}"));
    }
    if scope != SearchScope::All {
        search_extra.push(format!("&tab={}", scope.as_str()));
    }
    // Discussion ordering rides the pager too, or page 2 of a "newest" search
    // silently reverts to relevance.
    if thread_sort != ThreadSearchSort::default() {
        search_extra.push(format!("&tsort={}", thread_sort.as_str()));
    }
    // The product-side filters as one query-string fragment. Built once and
    // used twice: the pager needs it, and so does every link that crosses into
    // the Products tab (the tab strip, "see all N products"). Two copies of
    // this list is how one of them ends up missing a parameter.
    let mut facet_params = String::new();
    if let Some(t) = q.product_type.as_deref().filter(|s| !s.is_empty()) {
        facet_params.push_str(&format!("&type={}", urlencoding::encode(t)));
    }
    if let Some(bid) = facets.brand_id {
        facet_params.push_str(&format!("&brand_id={bid}"));
    }
    if let Some(mid) = facets.material_id {
        facet_params.push_str(&format!("&material_id={mid}"));
    }
    if let Some(pc) = q.pcat.as_deref().filter(|s| !s.is_empty()) {
        facet_params.push_str(&format!("&pcat={}", urlencoding::encode(pc)));
    }
    if facets.sort != ProductSearchSort::default() {
        facet_params.push_str(&format!("&psort={}", facets.sort.as_str()));
    }

    search_extra.push(facet_params.clone());
    let search_extra_params = search_extra.join("");

    // Links that carry the query across tabs — built here so the template does
    // no URL assembly, and so losing `q` when switching tabs is impossible.
    let q_param = urlencoding::encode(&query).to_string();
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("query", &query);
    ctx.insert("query_encoded", &q_param);
    ctx.insert("results", &results);
    ctx.insert("products", &products);
    ctx.insert("thread_total", &outcome.thread_total);
    ctx.insert("product_total", &outcome.product_total);
    ctx.insert("total_all", &(outcome.thread_total + outcome.product_total));
    ctx.insert("active_tab", scope.as_str());
    ctx.insert("search_error", &search_error);
    ctx.insert(
        "pagination",
        &PaginationCtx::new(page, PER_PAGE, paged_total, search_extra_params),
    );
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("search_categories", &all_categories);
    ctx.insert("active_category_id", &category_id.map(|id| id.to_string()));

    // Facet options are only fetched for the tab that shows them — two extra
    // queries on every discussion search would be pure waste.
    // Every active filter as a param the reader set, paired with the label to
    // show for it. Used twice: to draw the removable chips, and to build each
    // chip's "everything except me" URL.
    //
    // Chips exist because four dropdowns cannot answer "what is narrowing this
    // page right now?" at a glance — you have to read all of them and notice
    // which are not on their default. The chips say it in one line and make
    // each one removable in a click.
    let mut active_filters: Vec<(&str, String, String, String)> = Vec::new();

    if scope != SearchScope::Threads {
        let product_categories = state.product.list_categories().await.unwrap_or_default();
        let (brands, materials) =
            tokio::join!(state.product.list_brands(), state.product.list_materials(None));
        let brands = brands.unwrap_or_default();
        let materials = materials.unwrap_or_default();

        if let Some(pc) = q.pcat.as_deref().filter(|s| !s.is_empty()) {
            if let Some(found) = product_categories
                .iter()
                .find(|c| c.id.to_string() == pc)
            {
                active_filters.push(("pcat", pc.to_string(), "name".into(), found.name.clone()));
            }
        }
        if let Some(t) = q.product_type.as_deref().filter(|s| !s.is_empty()) {
            // The label is a translated enum, so the code travels and the
            // template resolves it through the shared `product_type_label`
            // macro — one spelling of these three words on the whole site.
            active_filters.push(("type", t.to_string(), "code".into(), t.to_string()));
        }
        if let Some(bid) = facets.brand_id {
            if let Some(found) = brands.iter().find(|b| b.id == bid) {
                active_filters.push((
                    "brand_id",
                    bid.to_string(),
                    "name".into(),
                    found.name.clone(),
                ));
            }
        }
        if let Some(mid) = facets.material_id {
            if let Some(found) = materials.iter().find(|m| m.id == mid) {
                active_filters.push((
                    "material_id",
                    mid.to_string(),
                    "name".into(),
                    found.name.clone(),
                ));
            }
        }

        ctx.insert(
            "product_categories",
            &product_categories
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id.to_string(),
                        "name": c.name,
                        "is_child": c.parent_id.is_some(),
                    })
                })
                .collect::<Vec<_>>(),
        );
        ctx.insert(
            "brands",
            &brands.into_iter().map(BrandResponse::from).collect::<Vec<_>>(),
        );
        ctx.insert(
            "materials",
            &materials
                .into_iter()
                .map(MaterialResponse::from)
                .collect::<Vec<_>>(),
        );
    }

    if let Some(cid) = category_id {
        if let Some(found) = all_categories.iter().find(|c| c.id == cid.to_string()) {
            active_filters.push((
                "category_id",
                cid.to_string(),
                "name".into(),
                found.name.clone(),
            ));
        }
    }

    // Each chip links to the same page with only its own parameter removed.
    let chips: Vec<serde_json::Value> = active_filters
        .iter()
        .map(|(key, _, kind, label)| {
            let mut url = format!("/search?q={}", urlencoding::encode(&query));
            if scope != SearchScope::All {
                url.push_str(&format!("&tab={}", scope.as_str()));
            }
            for (other_key, other_value, _, _) in &active_filters {
                if other_key != key {
                    url.push_str(&format!(
                        "&{}={}",
                        other_key,
                        urlencoding::encode(other_value)
                    ));
                }
            }
            // Ordering is not filtering, so it survives every chip removal.
            if facets.sort != ProductSearchSort::default() {
                url.push_str(&format!("&psort={}", facets.sort.as_str()));
            }
            serde_json::json!({ "kind": kind, "label": label, "remove_url": url })
        })
        .collect();

    ctx.insert("filter_chips", &chips);
    ctx.insert("active_type", &q.product_type);
    ctx.insert("active_brand", &facets.brand_id.map(|id| id.to_string()));
    ctx.insert("active_material", &facets.material_id.map(|id| id.to_string()));
    ctx.insert("active_psort", facets.sort.as_str());
    ctx.insert("active_tsort", thread_sort.as_str());
    ctx.insert("active_pcat", &q.pcat);
    ctx.insert("product_filters_active", &facets.is_narrowed());
    ctx.insert("facet_params", &facet_params);
    // What the reader would get back by dropping one filter. Only populated on
    // a miss, so the template can offer a specific way out instead of a shrug.
    ctx.insert("relaxed", &outcome.relaxed);
    ctx.insert("has_relaxation", &outcome.relaxed.has_any());
    // A query that finds no product is a gap in the catalogue, not a dead end —
    // offer the contributor the one action that fixes it.
    ctx.insert(
        "can_submit_product",
        &auth_user
            .as_ref()
            .map_or(false, |u| PermissionChecker::can_submit_products(u).is_ok()),
    );

    render_with_theme_in(&state, &req_locale, &active, "search.html", &ctx).await
}
