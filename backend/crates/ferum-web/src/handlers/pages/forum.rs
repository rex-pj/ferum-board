use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use ferum_domain::models::ThreadStatus;
use ferum_domain::models::product::ProductStatus;
use ferum_domain::repositories::product_repository::{ProductListFilter, ProductSort};
use ferum_domain::repositories::thread_repository::ThreadSort;
use crate::view_models::page_context::{
    CategoryCtx, PaginationCtx, PostCtx, ReactionSummaryCtx, TagCtx, ThreadDetailCtx,
    ForumGroupCtx,
};
use crate::view_models::product::{ProductResponse, RatingStatsResponse};
use ferum_domain::models::reaction::ReactionKind;

use ferum_application::permission::PermissionChecker;
use super::{active_theme, map_threads, map_threads_with_ratings, review_overall_map, review_product_image_map, nav_categories_ctx, post_policy_str, view_policy_str, render_with_theme_in, user_ctx, PageError};

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
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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

    // ThreadUseCase::list_feed silently personalizes this feed to the viewer's
    // watched categories (minus muted) when any are set — surfaced here purely
    // for template transparency ("why does my feed look different?"), not to
    // drive the filtering itself.
    let watched_count = match auth_user.as_ref() {
        Some(u) => state
            .user_repo
            .get_watched_categories(u.id)
            .await
            .map(|v| v.len())
            .unwrap_or(0),
        None => 0,
    };

    let mut extra_parts: Vec<String> = Vec::new();
    if let Some(tag) = &q.tag {
        extra_parts.push(format!("&tag={tag}"));
    }
    if sort_str != "latest" {
        extra_parts.push(format!("&sort={sort_str}"));
    }
    let extra_params = extra_parts.join("");

    // ── Homepage market data ────────────────────────────────────────────────
    // Only on the primary view (no tag filter, first page) — pagination and tag
    // drill-downs shouldn't re-render the product strip. All failures degrade to
    // an empty band rather than failing the page.
    let show_market = q.tag.is_none() && page == 1;
    let (top_rated, total_products, total_reviews, avg_rating) = if show_market {
        let (items, product_total) = state
            .product
            .list(
                ProductListFilter {
                    status: Some(ProductStatus::Published),
                    sort: ProductSort::TopRated,
                    ..Default::default()
                },
                1,
                8,
            )
            .await
            .unwrap_or_default();
        // The strip is a "top rated" shelf — only products that actually carry a
        // score belong on it (TopRated already sorts the unrated last).
        let top: Vec<ProductResponse> = items
            .into_iter()
            .filter(|it| it.review_count > 0)
            .map(ProductResponse::from)
            .collect();
        let (review_count, avg) = state.review.global_stats().await.unwrap_or((0, None));
        (top, product_total, review_count, avg)
    } else {
        (Vec::new(), 0, 0, None)
    };

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    let thread_ratings = review_overall_map(&state, &threads).await;
    let thread_images = review_product_image_map(&state, &threads).await;
    ctx.insert("threads", &map_threads_with_ratings(&threads, &thread_ratings, &thread_images));
    ctx.insert("pagination", &PaginationCtx::new(page, per_page, total, extra_params));
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("total_threads", &total_threads);
    ctx.insert("total_categories", &total_categories);
    ctx.insert("active_tag", &q.tag);
    ctx.insert("active_sort", &sort_str);
    ctx.insert("watched_count", &watched_count);
    ctx.insert("top_rated", &top_rated);
    ctx.insert("total_products", &total_products);
    ctx.insert("total_reviews", &total_reviews);
    ctx.insert("avg_rating", &avg_rating);

    render_with_theme_in(&state, &req_locale, &active, "home.html", &ctx).await
}

#[tracing::instrument(skip_all)]
pub async fn forum_index(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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
                view_policy: view_policy_str(&item.category.view_policy),
                post_policy: post_policy_str(&item.category.post_policy),
                can_post: false,
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
                    view_policy: view_policy_str(&s.category.view_policy),
                    post_policy: post_policy_str(&s.category.post_policy),
                    can_post: false,
                })
                .collect();
            let recent_threads = map_threads(&item.recent_threads);
            ForumGroupCtx { parent, children, recent_threads }
        })
        .collect();

    let nav_categories = nav_categories_ctx(&state, auth_user.as_ref()).await;

    let total_threads: u64 = groups
        .iter()
        .map(|g| g.parent.thread_count + g.children.iter().map(|c| c.thread_count).sum::<u64>())
        .sum();
    let total_categories: usize =
        groups.iter().map(|g| 1 + g.children.len()).sum();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("groups", &groups);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("total_threads", &total_threads);
    ctx.insert("total_categories", &total_categories);

    render_with_theme_in(&state, &req_locale, &active, "forum/index.html", &ctx).await
}

#[tracing::instrument(skip(state, auth_user, q), fields(slug))]
pub async fn category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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
            view_policy: view_policy_str(&c.view_policy),
            post_policy: post_policy_str(&c.post_policy),
            can_post: false,
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
            view_policy: view_policy_str(&c.view_policy),
            post_policy: post_policy_str(&c.post_policy),
            can_post: false,
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
        view_policy: view_policy_str(&category.view_policy),
        post_policy: post_policy_str(&category.post_policy),
        can_post: PermissionChecker::user_can_create_post(auth_user.as_ref(), &category),
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
            view_policy: view_policy_str(&c.view_policy),
            post_policy: post_policy_str(&c.post_policy),
            can_post: false,
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
    let thread_ratings = review_overall_map(&state, &threads).await;
    let thread_images = review_product_image_map(&state, &threads).await;
    ctx.insert("threads", &map_threads_with_ratings(&threads, &thread_ratings, &thread_images));
    ctx.insert("pagination", &PaginationCtx::new(page, per_page, total, extra_params));
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("active_sort", &sort_str);
    ctx.insert("active_category_slug", &category.slug);

    render_with_theme_in(&state, &req_locale, &active, "forum/category.html", &ctx).await
}

#[tracing::instrument(skip(state, auth_user, headers, q), fields(slug))]
pub async fn thread_detail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
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

    // Server-computed capability flags — templates must never re-derive these
    // from usernames or role names (mirrors the pin/lock/move pattern).
    // See thread_permissions.rs for the pure, unit-tested logic.
    use super::thread_permissions::{
        post_can_delete, post_can_edit, thread_can_delete, thread_can_edit,
        thread_can_mark_best_answer,
    };
    let has_in_cat = |key: &str| {
        auth_user
            .as_ref()
            .is_some_and(|u| u.has_perm_in(key, thread.category_id))
    };
    let is_locked = matches!(thread.status, ThreadStatus::Locked);
    use ferum_domain::models::role::perm;

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
        excerpt: thread.excerpt.clone(),
        category_id: category.id.to_string(),
        category_slug: category.slug.clone(),
        category_name: category.name.clone(),
        category_description: category.description.clone(),
        reply_count: thread.reply_count,
        view_count: thread.view_count,
        is_pinned: thread.is_pinned,
        is_solved: thread.is_solved,
        is_locked,
        best_answer_id: thread.best_answer_id.map(|id| id.to_string()),
        created_at: thread.created_at.to_rfc3339(),
        can_pin: has_in_cat(perm::THREAD_PIN),
        can_lock: has_in_cat(perm::THREAD_LOCK),
        can_move: has_in_cat(perm::THREAD_MOVE),
        can_edit: thread_can_edit(auth_user.as_ref(), thread.author_id, thread.category_id, is_locked),
        can_delete: thread_can_delete(auth_user.as_ref(), thread.author_id, thread.category_id),
        can_mark_best_answer: thread_can_mark_best_answer(auth_user.as_ref(), thread.author_id, thread.category_id),
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
                let is_own = auth_user.as_ref().is_some_and(|u| u.id == p.author_id);
                let is_deleted = p.is_deleted;
                let is_pending = p.status.is_pending();
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
                    // Deleted content is never sent to the client, regardless of viewer.
                    content_md: if is_deleted { String::new() } else { p.content_md.clone() },
                    content_html: if is_deleted { String::new() } else { p.content_html.clone() },
                    created_at: p.created_at.to_rfc3339(),
                    edited_at: p.edited_at.map(|d| d.to_rfc3339()),
                    is_best_answer: Some(p.id) == thread.best_answer_id,
                    reactions: {
                        let for_kind = |kind: ReactionKind| {
                            let count = p.reactions.iter().find(|(k, _)| *k == kind).map(|(_, c)| *c as i64).unwrap_or(0);
                            let reacted = p.my_reactions.contains(&kind);
                            crate::view_models::page_context::ReactionKindCtx { count, reacted }
                        };
                        ReactionSummaryCtx {
                            like: for_kind(ReactionKind::Like),
                            helpful: for_kind(ReactionKind::Helpful),
                            insightful: for_kind(ReactionKind::Insightful),
                            funny: for_kind(ReactionKind::Funny),
                        }
                    },
                    is_deleted,
                    is_pending,
                    is_own,
                    can_edit: !is_deleted && post_can_edit(auth_user.as_ref(), p.author_id, thread.category_id),
                    can_delete: !is_deleted && post_can_delete(auth_user.as_ref(), p.author_id, thread.category_id),
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
            view_policy: view_policy_str(&c.view_policy),
            post_policy: post_policy_str(&c.post_policy),
            can_post: false,
        })
        .collect();

    let can_post = PermissionChecker::user_can_create_post(auth_user.as_ref(), &category);
    let can_upload_attachment = auth_user
        .as_ref()
        .is_some_and(|u| PermissionChecker::can_upload(u).is_ok());

    // If this thread reviews a product, surface it as a card — flagged "pending
    // approval" while the product is still a draft, so a public review of a
    // not-yet-approved submission reads clearly rather than 404-ing on the link.
    let review_product = match state.review.product_for_thread(thread.id).await {
        Ok(Some(pid)) => match state.product.get_by_id(pid).await {
            Ok(product) => {
                let stats = state
                    .review
                    .product_stats(pid)
                    .await
                    .ok()
                    .flatten()
                    .map(RatingStatsResponse::from);
                Some(serde_json::json!({
                    "slug": product.slug,
                    "name": product.name,
                    "primary_image_key": product.primary_image_key,
                    "is_published": product.status == ProductStatus::Published,
                    "avg_overall": stats.as_ref().and_then(|s| s.avg_overall),
                    "review_count": stats.as_ref().map(|s| s.review_count).unwrap_or(0),
                }))
            }
            Err(_) => None,
        },
        _ => None,
    };
    let is_review = review_product.is_some();

    // The OP's own structured rating — the spine of a review view. May be absent
    // if the review thread exists but its rating failed to save (rare).
    let review_rating = if is_review {
        state.review.get_rating(thread.id).await.ok().flatten().map(|r| {
            serde_json::json!({
                "overall": r.overall,
                "durability": r.durability,
                "materials": r.materials,
                "comfort": r.comfort,
                "aesthetics": r.aesthetics,
                "value_for_money": r.value_for_money,
                "verified_purchase": r.verified_purchase,
            })
        })
    } else {
        None
    };

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await);
    ctx.insert("active_theme", &active);
    ctx.insert("thread", &thread_ctx);
    ctx.insert("pagination", &pagination);
    ctx.insert("nav_categories", &nav_categories);
    ctx.insert("move_categories", &move_categories);
    ctx.insert("active_category_slug", &category.slug);
    ctx.insert("can_post", &can_post);
    ctx.insert("can_upload_attachment", &can_upload_attachment);
    ctx.insert("review_product", &review_product);
    ctx.insert("review_rating", &review_rating);
    ctx.insert("is_review", &is_review);

    render_with_theme_in(&state, &req_locale, &active, "forum/thread.html", &ctx).await
}

/// GET /go/post/{id} — resolves a post to the thread page + pagination offset
/// it actually renders on, so notification links and the best-answer badge
/// don't always dump the visitor on page 1 to hunt for the post themselves.
#[tracing::instrument(skip(state, auth_user), fields(post_id = %post_id))]
pub async fn goto_post(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(post_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, PageError> {
    let (slug, page) = state.post.locate_post(auth_user.as_ref(), post_id).await?;
    let target = if page > 1 {
        format!("/forum/t/{slug}?page={page}#post-{post_id}")
    } else {
        format!("/forum/t/{slug}#post-{post_id}")
    };
    Ok(axum::response::Redirect::to(&target))
}
