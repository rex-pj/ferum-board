pub mod account;
pub mod auth;
pub mod catalog;
pub mod compose;
pub mod forum;
pub mod profile;
pub mod search;
pub mod setup;
pub mod sitemap;
pub mod thread_permissions;

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use ferum_application::constants::DEFAULT_THEME_SLUG;
use ferum_application::shared::AppError;
use ferum_domain::models::{PostPolicy, ViewPolicy};
use crate::view_models::page_context::{
    CurrentUserCtx, NavCategoryCtx, PluginSlotCtx, TagCtx, ThreadCtx,
};

// ─── Shared query types ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PageQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

pub fn post_policy_str(p: &PostPolicy) -> String {
    match p {
        PostPolicy::Members   => "members",
        PostPolicy::Trusted   => "trusted",
        PostPolicy::StaffOnly => "staff_only",
        PostPolicy::Closed    => "closed",
        PostPolicy::Moderated => "moderated",
    }
    .to_string()
}

pub fn view_policy_str(p: &ViewPolicy) -> String {
    match p {
        ViewPolicy::Public      => "public",
        ViewPolicy::MembersOnly => "members_only",
        ViewPolicy::StaffOnly   => "staff_only",
    }
    .to_string()
}

/// Unwraps `Option<AuthUser>` for page handlers — redirects to `/login` on failure.
pub fn require_page_auth(auth_user: Option<AuthUser>) -> Result<AuthUser, PageError> {
    auth_user.ok_or(PageError::Unauthorized)
}

pub(super) async fn active_theme(state: &AppState) -> String {
    let _lat = crate::telemetry::Latency::start("active_theme");
    let slug = state.active_theme_cache.read().await.clone();
    crate::telemetry::record_cache_result("active_theme", "rwlock", true);
    slug
}

pub(super) async fn user_ctx(
    state: &AppState,
    auth_user: Option<&AuthUser>,
) -> Option<CurrentUserCtx> {
    let u = auth_user?;
    let (unread, prefs) = tokio::join!(
        state.notification.unread_count(u),
        state.user.get_preferences_by_id(u.id),
    );
    let mut ctx = CurrentUserCtx::from_auth(u, unread.unwrap_or(0));
    if let Ok(p) = prefs {
        ctx.theme = p.theme;
        ctx.font_size = p.font_size;
        ctx.layout = p.layout;
    }
    Some(ctx)
}

#[tracing::instrument(skip_all)]
pub async fn nav_categories_ctx(
    state: &AppState,
    auth_user: Option<&AuthUser>,
) -> Vec<NavCategoryCtx> {
    let cats = state
        .category
        .list_visible(auth_user)
        .await
        .unwrap_or_default();

    let mut children_map: std::collections::HashMap<String, Vec<NavCategoryCtx>> =
        std::collections::HashMap::new();
    let mut parents: Vec<(String, NavCategoryCtx)> = Vec::new();

    for c in &cats {
        let entry = NavCategoryCtx {
            slug: c.slug.clone(),
            name: c.name.clone(),
            color: c.color.clone(),
            children: Vec::new(),
        };
        if let Some(pid) = c.parent_id {
            children_map.entry(pid.to_string()).or_default().push(entry);
        } else {
            parents.push((c.id.to_string(), entry));
        }
    }

    parents
        .into_iter()
        .map(|(id, mut p)| {
            p.children = children_map.remove(&id).unwrap_or_default();
            p
        })
        .collect()
}

pub(super) async fn plugin_ctx_data(
    state: &AppState,
) -> (
    std::collections::HashMap<String, Vec<PluginSlotCtx>>,
    Vec<String>,
) {
    let slots = state.plugin_ui.active_ui_slots().await;
    let mut map: std::collections::HashMap<String, Vec<PluginSlotCtx>> =
        std::collections::HashMap::new();
    let mut assets: Vec<String> = Vec::new();
    let mut seen_assets: std::collections::HashSet<String> = std::collections::HashSet::new();
    for slot in slots {
        let attrs = slot.props.join(" ");
        let html = if attrs.is_empty() {
            format!("<{0}></{0}>", slot.custom_element_tag)
        } else {
            format!("<{0} {1}></{0}>", slot.custom_element_tag, attrs)
        };
        if seen_assets.insert(slot.asset_url.clone()) {
            assets.push(slot.asset_url);
        }
        map.entry(slot.slot_name)
            .or_default()
            .push(PluginSlotCtx { html });
    }
    (map, assets)
}

#[tracing::instrument(skip(state, ctx), fields(theme = active_theme, page_key))]
pub async fn render_with_theme(
    state: &AppState,
    active_theme: &str,
    page_key: &str,
    ctx: &Context,
) -> Result<Html<String>, PageError> {
    let (plugin_slots, plugin_assets) = plugin_ctx_data(state).await;
    let theme_bs_theme = state.active_theme_color_scheme_cache.read().await.clone();
    let mut ctx = ctx.clone();
    ctx.insert("plugin_slots", &plugin_slots);
    ctx.insert("plugin_assets", &plugin_assets);
    ctx.insert("theme_bs_theme", &theme_bs_theme);

    let chain = state.active_theme_chain_cache.read().await.clone();
    let candidates: Vec<String> = chain
        .iter()
        .map(|slug| format!("{}/templates/{}", slug, page_key))
        .collect();
    let template_name = state
        .tera
        .first_existing_template(&candidates)
        .await
        .unwrap_or_else(|| format!("{}/templates/{}", DEFAULT_THEME_SLUG, page_key));

    let html = state.tera.render(&template_name, &ctx).await?;
    Ok(Html(html))
}

pub(super) fn map_threads(
    threads: &[ferum_domain::models::Thread],
) -> Vec<ThreadCtx> {
    let empty_r = std::collections::HashMap::new();
    let empty_i = std::collections::HashMap::new();
    map_threads_with_ratings(threads, &empty_r, &empty_i)
}

/// Like `map_threads`, but attaches each review's overall score and product
/// cover from pre-loaded `thread_id → …` maps (batch-fetched by the caller —
/// no N+1).
pub(super) fn map_threads_with_ratings(
    threads: &[ferum_domain::models::Thread],
    ratings: &std::collections::HashMap<uuid::Uuid, i16>,
    product_images: &std::collections::HashMap<uuid::Uuid, String>,
) -> Vec<ThreadCtx> {
    threads
        .iter()
        .map(|t| {
            let author = t.author_username.clone().unwrap_or_default();
            ThreadCtx {
                id: t.id.to_string(),
                slug: t.slug.clone(),
                title: t.title.clone(),
                author_username: author.clone(),
                author_display_name: t
                    .author_display_name
                    .clone()
                    .unwrap_or_else(|| author.clone()),
                author_avatar_url: t.author_avatar_url.clone(),
                category_slug: t.category_slug.clone(),
                category_name: t.category_name.clone().unwrap_or_default(),
                is_review: t.category_slug
                    == ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG,
                review_overall: ratings.get(&t.id).copied(),
                review_product_image: product_images.get(&t.id).cloned(),
                reply_count: t.reply_count,
                view_count: t.view_count,
                is_pinned: t.is_pinned,
                is_solved: t.is_solved,
                is_locked: matches!(t.status, ferum_domain::models::ThreadStatus::Locked),
                status: format!("{:?}", t.status).to_lowercase(),
                thumbnail_url: t.thumbnail_url.clone(),
                excerpt: t.excerpt.clone(),
                last_post_at: t.last_post_at.map(|d| d.to_rfc3339()),
                created_at: t.created_at.to_rfc3339(),
                tags: t
                    .tags
                    .iter()
                    .map(|tag| TagCtx {
                        name: tag.name.clone(),
                        slug: tag.slug.clone(),
                        color: tag.color.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Batch-load the overall score for every review thread in `threads`, keyed by
/// thread id. One query for the whole page (no N+1); empty when none are reviews.
pub(super) async fn review_overall_map(
    state: &crate::app_state::AppState,
    threads: &[ferum_domain::models::Thread],
) -> std::collections::HashMap<uuid::Uuid, i16> {
    use ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG;
    let ids: Vec<uuid::Uuid> = threads
        .iter()
        .filter(|t| t.category_slug == REVIEWS_CATEGORY_SLUG)
        .map(|t| t.id)
        .collect();
    if ids.is_empty() {
        return std::collections::HashMap::new();
    }
    state
        .review
        .ratings_for_threads(&ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(id, r)| (id, r.overall))
        .collect()
}

/// Batch-load `review_thread_id → product cover key` for the review threads in
/// `threads`, so review cards can fall back to the product image. One query;
/// empty when none are reviews.
pub(super) async fn review_product_image_map(
    state: &crate::app_state::AppState,
    threads: &[ferum_domain::models::Thread],
) -> std::collections::HashMap<uuid::Uuid, String> {
    use ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG;
    let ids: Vec<uuid::Uuid> = threads
        .iter()
        .filter(|t| t.category_slug == REVIEWS_CATEGORY_SLUG)
        .map(|t| t.id)
        .collect();
    if ids.is_empty() {
        return std::collections::HashMap::new();
    }
    state.product.review_thumbnails(&ids).await.unwrap_or_default()
}

// ─── Static fallback HTML ─────────────────────────────────────────────────────
// Used when Tera is unavailable (e.g. during a panic or DB-down scenario).
// Never contains user input — safe to serve unconditionally.

static STATIC_404_HTML: &str = include_str!("../../error_pages/404_static.html");
static STATIC_ERROR_HTML: &str = include_str!("../../error_pages/error_static.html");

// ─── Error page render helpers ────────────────────────────────────────────────

pub async fn render_404_page(state: &AppState, auth_user: Option<&AuthUser>) -> Response {
    let active = active_theme(state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &crate::handlers::admin::site_ctx(state).await);
    ctx.insert("active_theme", &active);
    ctx.insert("current_user", &user_ctx(state, auth_user).await);
    match render_with_theme(state, &active, "errors/404.html", &ctx).await {
        Ok(html) => (StatusCode::NOT_FOUND, html).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Html(STATIC_404_HTML)).into_response(),
    }
}

pub async fn render_error_page(state: &AppState, auth_user: Option<&AuthUser>) -> Response {
    let active = active_theme(state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &crate::handlers::admin::site_ctx(state).await);
    ctx.insert("active_theme", &active);
    ctx.insert("current_user", &user_ctx(state, auth_user).await);
    match render_with_theme(state, &active, "errors/error.html", &ctx).await {
        Ok(html) => (StatusCode::INTERNAL_SERVER_ERROR, html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Html(STATIC_ERROR_HTML)).into_response(),
    }
}

// ─── Error type ───────────────────────────────────────────────────────────────

pub enum PageError {
    Internal(anyhow::Error),
    Unauthorized,
    NotFound,
}

impl From<anyhow::Error> for PageError {
    fn from(e: anyhow::Error) -> Self {
        PageError::Internal(e)
    }
}

impl From<AppError> for PageError {
    fn from(e: AppError) -> Self {
        PageError::Internal(anyhow::anyhow!("{:?}", e))
    }
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        match self {
            PageError::Internal(e) => {
                tracing::error!("Page render error: {:?}", e);
                // Never render via Tera here — Tera itself may have caused this error.
                (StatusCode::INTERNAL_SERVER_ERROR, Html(STATIC_ERROR_HTML)).into_response()
            }
            PageError::Unauthorized => {
                axum::response::Redirect::to("/login").into_response()
            }
            PageError::NotFound => {
                // No AppState here; static fallback is acceptable for in-handler NotFound.
                (StatusCode::NOT_FOUND, Html(STATIC_404_HTML)).into_response()
            }
        }
    }
}
