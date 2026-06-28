use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use ferum_domain::models::ThreadStatus;
use ferum_domain::repositories::thread_repository::ThreadSort;
use crate::view_models::page_context::{
    CategoryCtx, PaginationCtx, PostCtx, ReactionSummaryCtx, TagCtx, ThreadDetailCtx,
    ForumGroupCtx,
};
use ferum_domain::models::reaction::ReactionKind;

use super::{active_theme, map_threads, nav_categories_ctx, post_policy_str, render_with_theme, user_ctx, PageError};

#[derive(Deserialize)]
pub struct ListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub tag: Option<String>,
    pub sort: Option<String>,
}

#[tracing::instrument(skip_all, fields(page = q.page, sort = q.sort.as_deref(), tag = q.tag.as_deref()))]
pub async fn home(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, PageError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let sort = ThreadSort::from_str(q.sort.as_deref().unwrap_or("latest"));
    let sort_str = sort.as_str().to_string();

    let (threads, total) = if let Some(tag_slug) = &q.tag {
        state
            .thread
            .list_by_tag(auth_user.as_ref(), tag_slug, sort, page, per_page)
            .await?
    } else {
        state
            .thread
            .list_feed(auth_user.as_ref(), sort, page, per_page)
            .await?
    };

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;
    let total_threads = total;
    let total_categories = nav_categories.len()
        + nav_categories.iter().map(|c| c.children.len()).sum::<usize>();

    let mut extra_parts: Vec<String> = Vec::new();
    if let Some(tag) = &q.tag {
        extra_parts.push(format!("&tag={tag}"));
    }
    if sort_str != "latest" {
        extra_parts.push(format!("&sort={sort_str}"));
    }
    let extra_params = extra_parts.join("");

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("threads", &map_threads(&threads));
    ctx.insert("pagination", &PaginationCtx::new(page, per_page, total, extra_params));
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("total_threads", &total_threads);
    ctx.insert("total_categories", &total_categories);
    ctx.insert("active_tag", &q.tag);
    ctx.insert("active_sort", &sort_str);

    render_with_theme(&state, &active, "home.html", &ctx).await
}

#[tracing::instrument(skip_all)]
pub async fn forum_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    let items = state.category.get_forum_index(auth_user.as_ref()).await?;
    let active = active_theme(&state).await;

    let groups: Vec<ForumGroupCtx> = items
        .into_iter()
        .map(|item| {
            let parent = CategoryCtx {
                id: item.category.id.to_string(),
                parent_id: item.category.parent_id.map(|id| id.to_string()),
                slug: item.category.slug.clone(),
                name: item.category.name.clone(),
                description: item.category.description.clone(),
                color: item.category.color.clone(),
                thread_count: item.thread_count,
                post_policy: post_policy_str(&item.category.post_policy),
            };
            let children: Vec<CategoryCtx> = item
                .subcategories
                .into_iter()
                .map(|s| CategoryCtx {
                    id: s.category.id.to_string(),
                    parent_id: Some(item.category.id.to_string()),
                    slug: s.category.slug.clone(),
                    name: s.category.name.clone(),
                    description: s.category.description.clone(),
                    color: s.category.color.clone(),
                    thread_count: s.thread_count,
                    post_policy: post_policy_str(&s.category.post_policy),
                })
                .collect();
            let recent_threads = map_threads(&item.recent_threads);
            ForumGroupCtx { parent, children, recent_threads }
        })
        .collect();

    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("groups", &groups);
    ctx.insert("nav_categories", &nav_categories);

    render_with_theme(&state, &active, "forum/index.html", &ctx).await
}

#[tracing::instrument(skip(state, auth_user, q), fields(slug))]
pub async fn category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, PageError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);
    let sort = ThreadSort::from_str(q.sort.as_deref().unwrap_or("latest"));
    let sort_str = sort.as_str().to_string();

    let category = state
        .category
        .get_by_slug(auth_user.as_ref(), &slug)
        .await?;

    let (threads, total) = state
        .thread
        .list_by_category(auth_user.as_ref(), &slug, sort, page, per_page)
        .await?;

    let active = active_theme(&state).await;
    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let all_categories = state
        .category
        .list_visible(auth_user.as_ref())
        .await
        .unwrap_or_default();

    let subcategories: Vec<CategoryCtx> = all_categories
        .iter()
        .filter(|c| c.parent_id == Some(category.id))
        .map(|c| CategoryCtx {
            id: c.id.to_string(),
            parent_id: Some(category.id.to_string()),
            slug: c.slug.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            color: c.color.clone(),
            thread_count: 0,
            post_policy: post_policy_str(&c.post_policy),
        })
        .collect();

    let sibling_categories: Vec<CategoryCtx> = all_categories
        .iter()
        .filter(|c| c.id != category.id && c.parent_id == category.parent_id)
        .take(5)
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

    let category_ctx = CategoryCtx {
        id: category.id.to_string(),
        parent_id: category.parent_id.map(|id| id.to_string()),
        slug: category.slug.clone(),
        name: category.name.clone(),
        description: category.description.clone(),
        color: category.color.clone(),
        thread_count: total,
        post_policy: post_policy_str(&category.post_policy),
    };

    let extra_params = if sort_str != "latest" {
        format!("&sort={sort_str}")
    } else {
        String::new()
    };

    let parent_category: Option<CategoryCtx> = category.parent_id.and_then(|parent_id| {
        all_categories.iter().find(|c| c.id == parent_id).map(|c| CategoryCtx {
            id: c.id.to_string(),
            parent_id: None,
            slug: c.slug.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            color: c.color.clone(),
            thread_count: 0,
            post_policy: post_policy_str(&c.post_policy),
        })
    });

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("category", &category_ctx);
    ctx.insert("parent_category", &parent_category);
    ctx.insert("subcategories", &subcategories);
    ctx.insert("sibling_categories", &sibling_categories);
    ctx.insert("threads", &map_threads(&threads));
    ctx.insert("pagination", &PaginationCtx::new(page, per_page, total, extra_params));
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("active_sort", &sort_str);
    ctx.insert("active_category_slug", &category.slug);

    render_with_theme(&state, &active, "forum/category.html", &ctx).await
}

#[tracing::instrument(skip(state, auth_user, headers, q), fields(slug))]
pub async fn thread_detail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, PageError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;

    let guest_fp = crate::utils::guest_fingerprint(&headers);
    let thread = state
        .thread
        .get_by_slug(auth_user.as_ref(), &slug, guest_fp.as_deref())
        .await?;

    let category = state
        .category
        .get_by_slug(auth_user.as_ref(), &thread.category_slug)
        .await?;

    let (posts, total) = state
        .post
        .list_by_thread(auth_user.as_ref(), thread.id, page, per_page)
        .await?;

    let active = active_theme(&state).await;

    let author_username = thread.author_username.clone().unwrap_or_default();
    let thread_ctx = ThreadDetailCtx {
        id: thread.id.to_string(),
        slug: thread.slug.clone(),
        title: thread.title.clone(),
        author_username: author_username.clone(),
        author_display_name: thread
            .author_display_name
            .clone()
            .unwrap_or_else(|| author_username.clone()),
        author_avatar_url: thread.author_avatar_url.clone(),
        thumbnail_url: thread.thumbnail_url.clone(),
        category_id: category.id.to_string(),
        category_slug: category.slug.clone(),
        category_name: category.name.clone(),
        category_description: category.description.clone(),
        reply_count: thread.reply_count,
        view_count: thread.view_count,
        is_pinned: thread.is_pinned,
        is_solved: thread.is_solved,
        is_locked: matches!(thread.status, ThreadStatus::Locked),
        created_at: thread.created_at.to_rfc3339(),
        tags: thread
            .tags
            .iter()
            .map(|t| TagCtx {
                name: t.name.clone(),
                slug: t.slug.clone(),
                color: t.color.clone(),
            })
            .collect(),
        posts: posts
            .iter()
            .map(|p| {
                let post_author = p.author_username.clone().unwrap_or_default();
                PostCtx {
                    id: p.id.to_string(),
                    author_username: post_author.clone(),
                    author_display_name: p
                        .author_display_name
                        .clone()
                        .unwrap_or_else(|| post_author.clone()),
                    author_avatar_url: p.author_avatar_url.clone(),
                    author_trust_level: p.author_role.clone().unwrap_or_default(),
                    content_md: p.content_md.clone(),
                    content_html: p.content_html.clone(),
                    created_at: p.created_at.to_rfc3339(),
                    edited_at: p.edited_at.map(|d| d.to_rfc3339()),
                    is_best_answer: Some(p.id) == thread.best_answer_id,
                    reactions: {
                        let count = |kind: ReactionKind| {
                            p.reactions.iter().find(|(k, _)| *k == kind).map(|(_, c)| *c as i64).unwrap_or(0)
                        };
                        ReactionSummaryCtx {
                            like: count(ReactionKind::Like),
                            helpful: count(ReactionKind::Helpful),
                            insightful: count(ReactionKind::Insightful),
                            funny: count(ReactionKind::Funny),
                        }
                    },
                }
            })
            .collect(),
        pagination: PaginationCtx::simple(page, per_page, total),
    };

    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;
    let pagination = thread_ctx.pagination.clone();

    let move_categories: Vec<CategoryCtx> = state
        .category
        .list_visible(auth_user.as_ref())
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.id != category.id)
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

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("thread", &thread_ctx);
    ctx.insert("pagination", &pagination);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("move_categories", &move_categories);
    ctx.insert("active_category_slug", &category.slug);

    render_with_theme(&state, &active, "forum/thread.html", &ctx).await
}
