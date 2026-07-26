use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{parse_date_from, parse_date_to, parse_opt_uuid, render_admin, require_admin, site_ctx, PageQuery, SelectOptionCtx};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, PaginationCtx, TagCtx, ThreadCtx};
use ferum_domain::repositories::thread_repository::ThreadSort;

#[tracing::instrument(skip_all, fields(page = q.page, search = q.q.as_deref(), status = q.status.as_deref()))]
pub async fn threads(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, 100);
    let search = q.q.clone().unwrap_or_default();
    let sort = ThreadSort::from_str(q.sort_by.as_deref().unwrap_or("latest"));
    let status_filter = q.status.clone().unwrap_or_default();
    let category_id = parse_opt_uuid(q.category_id.as_deref());
    let author_id = parse_opt_uuid(q.author_id.as_deref());
    let date_from = q.date_from.clone().unwrap_or_default();
    let date_to = q.date_to.clone().unwrap_or_default();

    let filter = ferum_domain::repositories::thread_repository::AdminThreadFilter {
        search: if search.is_empty() { None } else { Some(search.clone()) },
        status: if status_filter.is_empty() { None } else { Some(status_filter.clone()) },
        category_id,
        author_id,
        created_from: parse_date_from(q.date_from.as_deref()),
        created_to: parse_date_to(q.date_to.as_deref()),
        sort,
    };

    let (threads, total) = state
        .thread
        .list_threads_for_admin(&auth_user, filter, page, per_page)
        .await?;

    let categories = state.admin.list_categories(&auth_user).await.unwrap_or_default();
    let category_options: Vec<SelectOptionCtx> = categories
        .iter()
        .map(|c| SelectOptionCtx { id: c.id.to_string(), name: c.name.clone() })
        .collect();

    let author_label = match author_id {
        Some(id) => state
            .admin
            .get_user(&auth_user, id)
            .await
            .ok()
            .map(|u| u.display_name.unwrap_or(u.username))
            .unwrap_or_default(),
        None => String::new(),
    };

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
            is_locked: matches!(
                t.status,
                ferum_domain::models::thread::ThreadStatus::Locked
            ),
            status: format!("{:?}", t.status).to_lowercase(),
            thumbnail_url: t.thumbnail_url.clone(),
            last_post_at: t.last_post_at.map(|d| d.to_rfc3339()),
            created_at: t.created_at.to_rfc3339(),
            excerpt: None,
            tags: t
                .tags
                .iter()
                .map(|tag| TagCtx {
                    name: tag.name.clone(),
                    slug: tag.slug.clone(),
                    color: tag.color.clone(),
                })
                .collect(),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("threads", &threads_ctx);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("search_query", &search);
    ctx.insert("status_filter", &status_filter);
    ctx.insert("sort_by", &q.sort_by.clone().unwrap_or_else(|| "latest".to_string()));
    ctx.insert("category_options", &category_options);
    ctx.insert("category_id_filter", &category_id.map(|c| c.to_string()).unwrap_or_default());
    ctx.insert("author_id_filter", &author_id.map(|a| a.to_string()).unwrap_or_default());
    ctx.insert("author_label", &author_label);
    ctx.insert("date_from_filter", &date_from);
    ctx.insert("date_to_filter", &date_to);

    render_admin(&state, &req_locale, "admin/threads.html", &ctx).await
}
