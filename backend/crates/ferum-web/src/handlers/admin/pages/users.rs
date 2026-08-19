use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx, PageQuery};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{
    AdminUserDetailCtx, AdminUserRowCtx, CurrentUserCtx, PaginationCtx, RoleCtx,
};

#[tracing::instrument(skip_all, fields(page = q.page, search = q.q.as_deref()))]
pub async fn users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, ferum_application::constants::MAX_LIST_PAGE_SIZE)?;
    let search = q.q.clone();
    let sort_by = q.sort_by.clone();
    let sort_dir = q.sort_dir.clone();

    let (users, total) = state
        .admin
        .list_users(
            &auth_user,
            page,
            per_page,
            search.as_deref(),
            sort_by.as_deref(),
            sort_dir.as_deref(),
        )
        .await?;

    let user_ids: Vec<uuid::Uuid> = users.iter().map(|u| u.id).collect();
    let assignments = state
        .role
        .list_role_assignments_for_users(&user_ids)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load role assignments for user list");
            vec![]
        });

    let mut roles_by_user: std::collections::HashMap<uuid::Uuid, Vec<RoleCtx>> =
        std::collections::HashMap::new();
    for a in &assignments {
        roles_by_user.entry(a.user_id).or_default().push(RoleCtx {
            id: a.role_id.to_string(),
            slug: a.role_slug.clone(),
            name: a.role_name.clone(),
            color: a.role_color.clone(),
            is_system: false,
            is_default: false,
            member_count: 0,
        });
    }

    let users_ctx: Vec<AdminUserRowCtx> = users
        .into_iter()
        .map(|u| {
            let roles = roles_by_user.remove(&u.id).unwrap_or_default();
            AdminUserRowCtx {
                display_name: u.display_name.clone().unwrap_or_else(|| u.username.clone()),
                id: u.id.to_string(),
                username: u.username,
                email: u.email,
                avatar_url: u.avatar_url,
                trust_level: format!("{:?}", u.trust_level).to_lowercase(),
                roles,
                is_banned: u.is_banned,
                created_at: u.created_at.to_rfc3339(),
            }
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("users", &users_ctx);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("search_query", &search.unwrap_or_default());
    ctx.insert("sort_by", &sort_by.unwrap_or_default());
    ctx.insert("sort_dir", &sort_dir.unwrap_or_else(|| "desc".to_string()));

    render_admin(&state, &req_locale, "admin/users/list.html", ctx).await
}

#[tracing::instrument(skip(state, auth_user), fields(user_id = %id))]
pub async fn user_detail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let user = state.admin.get_user(&auth_user, id).await?;
    let all_roles = state.role.list_roles().await?;

    let all_roles_ctx: Vec<RoleCtx> = all_roles
        .iter()
        .map(|r| RoleCtx {
            id: r.id.to_string(),
            slug: r.slug.clone(),
            name: r.name.clone(),
            color: r.color.clone(),
            is_system: r.is_system,
            is_default: r.is_default,
            member_count: 0,
        })
        .collect();

    let assignments = state.role.list_user_roles(&auth_user, id).await.unwrap_or_default();
    let assigned_roles: Vec<RoleCtx> = assignments
        .iter()
        .filter_map(|a| {
            all_roles.iter().find(|r| r.id == a.role_id).map(|r| RoleCtx {
                id: r.id.to_string(),
                slug: r.slug.clone(),
                name: r.name.clone(),
                color: r.color.clone(),
                is_system: r.is_system,
                is_default: r.is_default,
                member_count: 0,
            })
        })
        .collect();

    let profile = AdminUserDetailCtx {
        id: user.id.to_string(),
        username: user.username.clone(),
        display_name: user.display_name.clone().unwrap_or_else(|| user.username.clone()),
        email: user.email.clone(),
        email_verified: user.is_email_verified,
        avatar_url: user.avatar_url.clone(),
        trust_level: format!("{:?}", user.trust_level).to_lowercase(),
        trust_score: user.trust_score,
        roles: assigned_roles,
        is_banned: user.is_banned,
        ban_reason: user.ban_reason.clone(),
        banned_until: user.banned_until.map(|d| d.to_rfc3339()),
        warn_count: user.warn_count,
        failed_login_count: user.failed_login_count,
        locked_until: user.locked_until.map(|d| d.to_rfc3339()),
        post_count: user.post_count,
        days_visited: user.days_visited,
        bio: user.bio.clone(),
        website: user.website.clone(),
        created_at: user.created_at.to_rfc3339(),
        last_seen_at: user.last_seen_at.map(|d| d.to_rfc3339()),
    };

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("profile", &profile);
    ctx.insert("all_roles", &all_roles_ctx);

    render_admin(&state, &req_locale, "admin/users/detail.html", ctx).await
}
