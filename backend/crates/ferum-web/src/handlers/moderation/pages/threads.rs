use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, PaginationCtx, TagCtx, ThreadCtx};
use ferum_domain::repositories::thread_repository::{AdminThreadFilter, ThreadSort};

use super::super::{require_moderator, ModPageQuery};

pub async fn threads(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;
    let status_filter = q.status.clone().unwrap_or_default();
    let search_query = q.q.clone().unwrap_or_default();

    let filter = AdminThreadFilter {
        search: if search_query.is_empty() { None } else { Some(search_query.clone()) },
        status: if status_filter.is_empty() { None } else { Some(status_filter.clone()) },
        sort: ThreadSort::default(),
        ..Default::default()
    };

    let (threads, total) = state
        .thread
        .list_threads_for_mod(&auth_user, filter, page, per_page)
        .await?;

    let threads_ctx: Vec<ThreadCtx> = threads
        .into_iter()
        .map(|t| ThreadCtx {
            id: t.id.to_string(),
            slug: t.slug.clone(),
            title: t.title.clone(),
            author_username: t.author_username.clone().unwrap_or_default(),
            author_display_name: t.author_display_name.clone().unwrap_or_default(),
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
            is_locked: matches!(t.status, ferum_domain::models::thread::ThreadStatus::Locked),
            status: format!("{:?}", t.status).to_lowercase(),
            thumbnail_url: t.thumbnail_url.clone(),
            excerpt: None,
            last_post_at: t.last_post_at.map(|d| d.to_rfc3339()),
            created_at: t.created_at.to_rfc3339(),
            tags: t.tags.iter().map(|tag| TagCtx {
                name: tag.name.clone(),
                slug: tag.slug.clone(),
                color: tag.color.clone(),
            }).collect(),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("threads", &threads_ctx);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("q", &search_query);
    ctx.insert("status", &status_filter);

    render_admin(&state, &req_locale, "mod/threads.html", ctx).await
}
