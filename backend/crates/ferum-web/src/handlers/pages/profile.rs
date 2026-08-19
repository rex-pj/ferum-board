use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{PaginationCtx, UserProfileCtx};

use super::{active_theme, map_threads, nav_categories_ctx, render_with_theme_in, user_ctx, PageError, PageQuery};

#[tracing::instrument(skip(state, auth_user, q), fields(username))]
pub async fn user_profile(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Path(username): Path<String>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let user = state
        .user_repo
        .find_by_username(&username)
        .await?
        .ok_or(PageError::NotFound)?;

    let active = active_theme(&state).await;

    let follow_status = state.follow.get_status(auth_user.as_ref().map(|u| u.id), user.id).await?;

    let (primary_role_slug, primary_role_name, primary_role_color) = {
        let assignments = state.user_role_repo.list_for_user(user.id).await.unwrap_or_default();
        let priority = ["admin", "moderator"];
        let primary = priority
            .iter()
            .find_map(|slug| assignments.iter().find(|a| a.category_id.is_none() && a.role_slug == *slug))
            .or_else(|| assignments.iter().find(|a| a.category_id.is_none() && a.role_slug != "member"));
        match primary {
            Some(a) => (Some(a.role_slug.clone()), Some(a.role_name.clone()), a.role_color.clone()),
            None => (None, None, None),
        }
    };

    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, state.thread.max_page_size().await)?;
    let (threads, total) = state.thread.list_by_author(auth_user.as_ref(), user.id, page, per_page).await?;

    let profile = UserProfileCtx {
        id: user.id.to_string(),
        username: user.username.clone(),
        display_name: user
            .display_name
            .clone()
            .unwrap_or_else(|| user.username.clone()),
        avatar_url: user.avatar_url.clone(),
        cover_url: user.cover_url.clone(),
        bio: user.bio.clone(),
        website: user.website.clone(),
        trust_level: format!("{:?}", user.trust_level).to_lowercase(),
        trust_score: Some(user.trust_score),
        primary_role_slug,
        primary_role_name,
        primary_role_color,
        post_count: user.post_count,
        follower_count: follow_status.follower_count,
        following_count: follow_status.following_count,
        created_at: user.created_at.to_rfc3339(),
        threads: map_threads(&threads),
        thread_pagination: Some(PaginationCtx::simple(page, per_page, total)),
    };

    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("profile", &profile);
    ctx.insert("nav_categories", &nav_categories);

    render_with_theme_in(&state, &req_locale, &active, "user/profile.html", ctx).await
}
