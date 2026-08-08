use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use std::collections::HashMap;
use tera::Context;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::pages::{post_policy_str, view_policy_str, PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CategoryCtx, CurrentUserCtx, PaginationCtx, QueuePostCtx};

use super::super::{require_moderator, ModPageQuery};

pub async fn queue(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;
    let category_id_str = q.category_id.clone().unwrap_or_default();
    let category_uuid: Option<Uuid> = category_id_str
        .parse::<Uuid>()
        .ok()
        .filter(|_| !category_id_str.is_empty());

    let (posts, total) = state
        .post
        .list_pending(&auth_user, category_uuid, page, per_page)
        .await?;

    // Batch-fetch author usernames.
    let mut author_names: HashMap<Uuid, String> = HashMap::new();
    for post in &posts {
        if let std::collections::hash_map::Entry::Vacant(e) = author_names.entry(post.author_id) {
            if let Ok(Some(u)) = state.user_repo.find_by_id(post.author_id).await {
                e.insert(u.username);
            }
        }
    }

    // Batch-fetch thread slugs + titles.
    let mut thread_info: HashMap<Uuid, (String, String)> = HashMap::new();
    for post in &posts {
        if let std::collections::hash_map::Entry::Vacant(e) = thread_info.entry(post.thread_id) {
            if let Ok(Some(t)) = state.thread.threads.find_by_id(post.thread_id).await {
                e.insert((t.slug, t.title));
            }
        }
    }

    let posts_ctx: Vec<QueuePostCtx> = posts
        .into_iter()
        .map(|p| {
            let author_username = author_names
                .get(&p.author_id)
                .cloned()
                .unwrap_or_default();
            let (thread_slug, thread_title) = thread_info
                .get(&p.thread_id)
                .cloned()
                .unwrap_or_default();
            QueuePostCtx {
                id: p.id.to_string(),
                author_username,
                thread_slug,
                thread_title,
                content_md: p.content_md,
                created_at: p.created_at.to_rfc3339(),
            }
        })
        .collect();

    // Load categories for filter dropdown.
    let all_categories = state
        .category
        .list_visible(Some(&auth_user))
        .await
        .unwrap_or_default();
    let categories_ctx: Vec<CategoryCtx> = all_categories
        .into_iter()
        .map(|c| CategoryCtx {
            id: c.id.to_string(),
            parent_id: c.parent_id.map(|id| id.to_string()),
            slug: c.slug,
            name: c.name,
            description: c.description,
            color: c.color,
            thread_count: 0,
            // The shared helpers, not `{:?}`: Debug yields "membersonly"/"staffonly",
            // which no template compares against.
            view_policy: view_policy_str(&c.view_policy),
            post_policy: post_policy_str(&c.post_policy),
            can_post: false,
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("posts", &posts_ctx);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("categories", &categories_ctx);
    ctx.insert("category_id_filter", &category_id_str);

    render_admin(&state, &req_locale, "mod/queue.html", ctx).await
}
