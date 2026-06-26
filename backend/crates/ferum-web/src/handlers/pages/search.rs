use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CategoryCtx, PaginationCtx, SearchHitCtx};

use super::{active_theme, nav_categories_ctx, post_policy_str, render_with_theme, user_ctx, PageError};

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub page: Option<u64>,
    pub category_id: Option<uuid::Uuid>,
}

#[tracing::instrument(skip(state, auth_user), fields(q = q.q.as_deref(), page = q.page))]
pub async fn search(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, PageError> {
    let active = active_theme(&state).await;
    let query = q.q.as_deref().unwrap_or_default().to_string();
    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;

    let search_results = if query.is_empty() {
        None
    } else {
        match state
            .search
            .search(query.clone(), q.category_id, page, per_page)
            .await
        {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::error!(error = %e, q = %query, "search_page_failed");
                None
            }
        }
    };

    let (hits, total) = match &search_results {
        Some(r) => (
            r.hits
                .iter()
                .map(|h| SearchHitCtx {
                    thread_slug: h.thread_slug.clone(),
                    thread_title: h.title.clone(),
                    excerpt: h.excerpt.clone(),
                })
                .collect::<Vec<_>>(),
            r.total,
        ),
        None => (vec![], 0),
    };

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
            post_policy: post_policy_str(&c.post_policy),
        })
        .collect();

    let mut search_extra: Vec<String> = Vec::new();
    if !query.is_empty() {
        search_extra.push(format!("&q={query}"));
    }
    if let Some(cid) = q.category_id {
        search_extra.push(format!("&category_id={cid}"));
    }
    let search_extra_params = search_extra.join("");

    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("query", &query);
    ctx.insert("results", &hits);
    ctx.insert("pagination", &PaginationCtx::new(page, per_page, total, search_extra_params));
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("search_categories", &all_categories);
    ctx.insert("active_category_id", &q.category_id.map(|id| id.to_string()));

    render_with_theme(&state, &active, "search.html", &ctx).await
}
