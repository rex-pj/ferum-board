use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::{
    AccountStatusCtx, NotificationCtx, PaginationCtx, TagCtx, ThreadCtx, UserProfileCtx,
};
use ferum_domain::models::ThreadStatus;

use super::{active_theme, login_redirect, nav_categories_ctx, render_with_theme_in, user_ctx, PageError, PageQuery};

pub async fn account(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(login_redirect("/account")),
    };

    let prefs = state
        .user
        .get_preferences(&auth_user)
        .await
        .unwrap_or_default();

    let (primary_role_slug, primary_role_name, primary_role_color) = {
        let assignments = state.user_role_repo.list_for_user(auth_user.id).await.unwrap_or_default();
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

    let user_row = state.user_repo.find_by_id(auth_user.id).await.ok().flatten();

    let profile = user_row.as_ref().map(|u| UserProfileCtx {
        id: u.id.to_string(),
        username: u.username.clone(),
        display_name: u.display_name.clone().unwrap_or_else(|| u.username.clone()),
        avatar_url: u.avatar_url.clone(),
        cover_url: u.cover_url.clone(),
        bio: u.bio.clone(),
        website: u.website.clone(),
        trust_level: format!("{:?}", u.trust_level).to_lowercase(),
        trust_score: Some(u.trust_score),
        primary_role_slug,
        primary_role_name,
        primary_role_color,
        post_count: u.post_count,
        follower_count: 0,
        following_count: 0,
        created_at: u.created_at.to_rfc3339(),
        threads: vec![],
        thread_pagination: None,
    });

    let account_status = user_row.as_ref().map(|u| AccountStatusCtx {
        is_banned: u.is_banned,
        ban_reason: u.ban_reason.clone(),
        banned_until: u.banned_until.map(|t| t.to_rfc3339()),
        warn_count: u.warn_count,
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
    ctx.insert("account_status", &account_status);
    ctx.insert("nav_categories", &nav_categories);

    render_with_theme_in(&state, &req_locale, &active, "app/account.html", ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn notifications(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(login_redirect("/notifications")),
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
            kind: n.kind.as_str().to_string(),
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

    render_with_theme_in(&state, &req_locale, &active, "app/notifications.html", ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn bookmarks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = match auth_user {
        Some(u) => u,
        None => return Ok(login_redirect("/bookmarks")),
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
                is_review: t.category_slug == ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG,
                review_overall: None,
                review_product_image: None,
                review_product_name: None,
                review_product_slug: None,
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

    render_with_theme_in(&state, &req_locale, &active, "app/bookmarks.html", ctx)
        .await
        .map(IntoResponse::into_response)
}
