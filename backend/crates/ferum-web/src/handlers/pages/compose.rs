use axum::extract::{Extension, Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CategoryCtx, TagCtx};
use ferum_application::permission::PermissionChecker;
use ferum_domain::models::product::ProductStatus;

use super::{active_theme, nav_categories_ctx, post_policy_str, view_policy_str, render_with_theme_in, user_ctx, PageError};

#[derive(Deserialize)]
pub struct NewThreadQuery {
    pub category_id: Option<String>,
    /// When set (e.g. arriving from a product page's "Write a review"), the
    /// picker is pre-filled and locked to this product, and the rating block
    /// is revealed immediately.
    pub product: Option<String>,
}

pub async fn new_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<NewThreadQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(Redirect::to("/login?next=/new-thread").into_response()),
    };

    let categories = state.category.list_visible(Some(&auth_user)).await?;
    // Only offer categories the user is actually allowed to post in — otherwise
    // picking a closed/staff_only/trust-gated category just leads to a submit-time 403.
    let categories_ctx: Vec<CategoryCtx> = categories
        .iter()
        .filter(|c| PermissionChecker::user_can_create_post(Some(&auth_user), c))
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
            can_post: true,
        })
        .collect();

    let can_upload_thumbnail = PermissionChecker::can_upload(&auth_user).is_ok();
    // Eligible to propose a brand-new catalog product (lands as draft for review).
    let can_submit_product = PermissionChecker::can_submit_products(&auth_user).is_ok();

    // Resolve an optional ?product=slug into a locked preselection. A missing or
    // unpublished product simply falls back to the normal (unlocked) picker.
    let preselected_product = match q.product.as_deref().filter(|s| !s.is_empty()) {
        Some(slug) => match state.product.get_by_slug(slug).await {
            Ok(p) if p.status == ProductStatus::Published => Some(serde_json::json!({
                "id": p.id.to_string(),
                "slug": p.slug,
                "name": p.name,
            })),
            _ => None,
        },
        None => None,
    };

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, Some(&auth_user)).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, Some(&auth_user)).await);
    ctx.insert("active_theme", &active);
    ctx.insert("categories", &categories_ctx);
    ctx.insert("preselected_category", &q.category_id.unwrap_or_default());
    ctx.insert("preselected_product", &preselected_product);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("can_upload_thumbnail", &can_upload_thumbnail);
    ctx.insert("can_submit_product", &can_submit_product);

    render_with_theme_in(&state, &req_locale, &active, "app/new_thread.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn edit_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(Redirect::to(&format!("/login?next=/edit-thread/{slug}")).into_response()),
    };

    let thread = state
        .thread
        .get_by_slug(Some(&auth_user), &slug, None)
        .await?;

    if thread.author_id != auth_user.id && !auth_user.has_perm("admin.users") {
        return Err(PageError::Unauthorized);
    }

    let first_post_content = state
        .post
        .list_by_thread(Some(&auth_user), thread.id, 1, 1)
        .await
        .ok()
        .and_then(|(posts, _)| posts.into_iter().next())
        .map(|p| p.content_md)
        .unwrap_or_default();

    let categories = state.category.list_visible(Some(&auth_user)).await?;
    let categories_ctx: Vec<CategoryCtx> = categories
        .iter()
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

    let thread_tags: Vec<TagCtx> = thread
        .tags
        .iter()
        .map(|tag| TagCtx {
            name: tag.name.clone(),
            slug: tag.slug.clone(),
            color: tag.color.clone(),
        })
        .collect();

    let can_upload_thumbnail = PermissionChecker::can_upload(&auth_user).is_ok();

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, Some(&auth_user)).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, Some(&auth_user)).await);
    ctx.insert("active_theme", &active);
    ctx.insert("thread_id", &thread.id.to_string());
    ctx.insert("thread_slug", &thread.slug);
    ctx.insert("thread_title", &thread.title);
    ctx.insert("thread_category_id", &thread.category_id.to_string());
    ctx.insert("thread_thumbnail_url", &thread.thumbnail_url);
    ctx.insert("first_post_content", &first_post_content);
    ctx.insert("thread_tags", &thread_tags);
    ctx.insert("categories", &categories_ctx);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("can_upload_thumbnail", &can_upload_thumbnail);

    render_with_theme_in(&state, &req_locale, &active, "app/edit_thread.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}
