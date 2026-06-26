use axum::extract::{Extension, Query, State};
use axum::response::{IntoResponse, Redirect};
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{
    NotificationCtx, PaginationCtx, TagCtx, ThreadCtx, UserProfileCtx,
};
use ferum_domain::models::ThreadStatus;

use super::{active_theme, nav_categories_ctx, render_with_theme, user_ctx, PageError, PageQuery};

pub async fn account(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(Redirect::to("/login?next=/account").into_response()),
    };

    let prefs = state
        .user
        .get_preferences(&auth_user)
        .await
        .unwrap_or_default();

    let profile = state
        .user_repo
        .find_by_id(auth_user.id)
        .await
        .ok()
        .flatten()
        .map(|u| UserProfileCtx {
            id: u.id.to_string(),
            username: u.username.clone(),
            display_name: u.display_name.clone().unwrap_or_else(|| u.username.clone()),
            avatar_url: u.avatar_url.clone(),
            cover_url: u.cover_url.clone(),
            bio: u.bio.clone(),
            website: u.website.clone(),
            trust_level: format!("{:?}", u.trust_level).to_lowercase(),
            trust_score: Some(u.trust_score),
            primary_role_slug: None,
            primary_role_name: None,
            primary_role_color: None,
            post_count: u.post_count,
            follower_count: 0,
            following_count: 0,
            created_at: u.created_at.to_rfc3339(),
            threads: vec![],
            thread_pagination: None,
        });

    let current_user = user_ctx(&state, Some(&auth_user)).await;
    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, Some(&auth_user)).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &current_user);
    ctx.insert("active_theme", &active);
    ctx.insert("preferences", &prefs);
    ctx.insert("profile", &profile);
    ctx.insert("nav_categories", &nav_categories);

    render_with_theme(&state, &active, "app/account.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn notifications(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(Redirect::to("/login?next=/notifications").into_response()),
    };

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;

    let (notifs, total) = state
        .notification
        .inbox(&auth_user, page, per_page)
        .await?;

    let notif_ctx: Vec<NotificationCtx> = notifs
        .iter()
        .map(|n| NotificationCtx {
            id: n.id.to_string(),
            kind: format!("{:?}", n.kind).to_lowercase(),
            payload: n.payload.clone(),
            is_read: n.is_read,
            created_at: n.created_at.to_rfc3339(),
        })
        .collect();

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, Some(&auth_user)).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, Some(&auth_user)).await);
    ctx.insert("active_theme", &active);
    ctx.insert("notifications", &notif_ctx);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("nav_categories", &nav_categories);

    render_with_theme(&state, &active, "app/notifications.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn bookmarks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(Redirect::to("/login?next=/bookmarks").into_response()),
    };

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;

    let (bmarks, total) = state.bookmark.list(&auth_user, page, per_page).await?;

    let threads: Vec<ThreadCtx> = bmarks
        .iter()
        .map(|(_, t)| {
            let author = t.author_username.clone().unwrap_or_default();
            ThreadCtx {
                id: t.id.to_string(),
                slug: t.slug.clone(),
                title: t.title.clone(),
                author_username: author.clone(),
                author_display_name: t.author_display_name.clone().unwrap_or_else(|| author.clone()),
                author_avatar_url: t.author_avatar_url.clone(),
                category_slug: t.category_slug.clone(),
                category_name: t.category_name.clone().unwrap_or_default(),
                reply_count: t.reply_count,
                view_count: t.view_count,
                is_pinned: t.is_pinned,
                is_solved: t.is_solved,
                is_locked: matches!(t.status, ThreadStatus::Locked),
                status: format!("{:?}", t.status).to_lowercase(),
                thumbnail_url: t.thumbnail_url.clone(),
                last_post_at: t.last_post_at.map(|d| d.to_rfc3339()),
                created_at: t.created_at.to_rfc3339(),
                excerpt: None,
                tags: t.tags.iter().map(|tag| TagCtx {
                    name: tag.name.clone(),
                    slug: tag.slug.clone(),
                    color: tag.color.clone(),
                }).collect(),
            }
        })
        .collect();

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, Some(&auth_user)).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, Some(&auth_user)).await);
    ctx.insert("active_theme", &active);
    ctx.insert("threads", &threads);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("nav_categories", &nav_categories);

    render_with_theme(&state, &active, "app/bookmarks.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}
