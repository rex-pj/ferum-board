use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx, ModeratorCtx, PageQuery};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AdminCategoryRowCtx, CurrentUserCtx};

#[tracing::instrument(skip_all, fields(search = q.q.as_deref()))]
pub async fn categories(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let search = q.q.clone().unwrap_or_default();
    let categories = state.admin.list_categories(&auth_user).await?;

    let id_to_name: std::collections::HashMap<String, String> = categories
        .iter()
        .map(|c| (c.id.to_string(), c.name.clone()))
        .collect();

    // Sort into tree order: roots first (by position), then each root's children (by position).
    // This ensures a parent always appears immediately before its children in the list.
    let mut roots: Vec<_> = categories.iter().filter(|c| c.parent_id.is_none()).collect();
    roots.sort_by_key(|c| c.position);
    let mut tree_ordered: Vec<_> = Vec::with_capacity(categories.len());
    for root in roots {
        tree_ordered.push(root);
        let mut children: Vec<_> = categories
            .iter()
            .filter(|c| c.parent_id == Some(root.id))
            .collect();
        children.sort_by_key(|c| c.position);
        tree_ordered.extend(children);
    }

    let search_lower = search.to_lowercase();
    let categories_ctx: Vec<AdminCategoryRowCtx> = tree_ordered
        .iter()
        .filter(|c| {
            search_lower.is_empty()
                || c.name.to_lowercase().contains(&search_lower)
                || c.slug.to_lowercase().contains(&search_lower)
        })
        .map(|c| {
            use ferum_domain::models::category::{PostPolicy, ViewPolicy};
            let parent_id = c.parent_id.map(|pid| pid.to_string());
            let parent_name = parent_id.as_deref().and_then(|pid| id_to_name.get(pid)).cloned();
            AdminCategoryRowCtx {
                id: c.id.to_string(),
                parent_id,
                parent_name,
                slug: c.slug.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                view_policy: match c.view_policy {
                    ViewPolicy::Public => "public",
                    ViewPolicy::MembersOnly => "members_only",
                    ViewPolicy::StaffOnly => "staff_only",
                }
                .to_string(),
                post_policy: match c.post_policy {
                    PostPolicy::Members => "members",
                    PostPolicy::Trusted => "trusted",
                    PostPolicy::StaffOnly => "staff_only",
                    PostPolicy::Closed => "closed",
                    PostPolicy::Moderated => "moderated",
                }
                .to_string(),
                color: c.color.clone(),
                position: c.position,
                thread_count: 0,
            }
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("categories", &categories_ctx);
    ctx.insert("search_query", &search);

    render_admin(&state, &req_locale, "admin/categories/list.html", ctx).await
}

pub async fn category_moderators(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let categories = state.admin.list_categories(&auth_user).await?;
    let category_name = categories
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| id.to_string());

    let raw_mods = state.admin.list_category_moderators(&auth_user, id).await?;
    let moderators: Vec<ModeratorCtx> = raw_mods
        .into_iter()
        .map(|(a, u)| ModeratorCtx {
            assignment_id: a.id.to_string(),
            user_id: u.id.to_string(),
            username: u.username.clone(),
            display_name: u.display_name.clone().unwrap_or_else(|| u.username.clone()),
            avatar_url: u.avatar_url.clone(),
            granted_at: a.created_at.to_rfc3339(),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("category_id", &id.to_string());
    ctx.insert("category_name", &category_name);
    ctx.insert("moderators", &moderators);

    render_admin(&state, &req_locale, "admin/categories/moderators.html", ctx).await
}
