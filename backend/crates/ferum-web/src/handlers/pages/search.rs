use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CategoryCtx, PaginationCtx, SearchHitCtx};

use super::{active_theme, nav_categories_ctx, post_policy_str, view_policy_str, render_with_theme, user_ctx, PageError};

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub page: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_optional_uuid")]
    pub category_id: Option<uuid::Uuid>,
}

fn deserialize_optional_uuid<'de, D>(deserializer: D) -> Result<Option<uuid::Uuid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    match s.as_deref() {
        None | Some("") => Ok(None),
        Some(v) => v.parse::<uuid::Uuid>().map(Some).map_err(serde::de::Error::custom),
    }
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

    let raw_categories = state
        .category
        .list_visible(auth_user.as_ref())
        .await
        .unwrap_or_default();

    // Resolve category filter: include the selected category plus all its children.
    let category_ids: Vec<uuid::Uuid> = match q.category_id {
        None => vec![],
        Some(selected_id) => {
            let mut ids = vec![selected_id];
            for cat in &raw_categories {
                if cat.parent_id == Some(selected_id) {
                    ids.push(cat.id);
                }
            }
            ids
        }
    };

    // Three distinct outcomes the template must be able to tell apart:
    // no query yet, a query with zero matches, and a query the search
    // backend failed to answer. Collapsing the last two into "no results"
    // tells the user their content doesn't exist when the engine is down.
    let mut search_error = false;
    let (hits, total) = if query.is_empty() {
        (vec![], 0)
    } else {
        match state
            .search
            .search_hydrated(query.clone(), category_ids, page, per_page)
            .await
        {
            Ok((hydrated, total)) => (
                hydrated
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
                    .collect::<Vec<_>>(),
                total,
            ),
            Err(e) => {
                tracing::error!(error = %e, q = %query, "search_page_failed");
                search_error = true;
                (vec![], 0)
            }
        }
    };

    let all_categories: Vec<CategoryCtx> = raw_categories
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
    ctx.insert("search_error", &search_error);
    ctx.insert("pagination", &PaginationCtx::new(page, per_page, total, search_extra_params));
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("search_categories", &all_categories);
    ctx.insert("active_category_id", &q.category_id.map(|id| id.to_string()));

    render_with_theme(&state, &active, "search.html", &ctx).await
}
