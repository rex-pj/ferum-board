use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Extension;
use serde::Serialize;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, PermissionCtx, RoleCtx};

#[tracing::instrument(skip_all)]
pub async fn roles(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let roles = state.role.list_roles().await?;
    let counts = state.role.count_members_by_role().await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to load member counts for roles list");
        std::collections::HashMap::new()
    });
    let roles_ctx: Vec<RoleCtx> = roles
        .iter()
        .map(|r| RoleCtx {
            id: r.id.to_string(),
            slug: r.slug.clone(),
            name: r.name.clone(),
            color: r.color.clone(),
            is_system: r.is_system,
            is_default: r.is_default,
            member_count: counts.get(&r.id).copied().unwrap_or(0),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("roles", &roles_ctx);

    render_admin(&state, &req_locale, "admin/roles/list.html", &ctx).await
}

#[tracing::instrument(skip(state, auth_user), fields(role_id = %id))]
pub async fn role_detail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let roles = state.role.list_roles().await?;
    let role = roles.iter().find(|r| r.id == id).ok_or_else(|| PageError::NotFound)?;
    let counts = state.role.count_members_by_role().await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to load member counts for role detail");
        std::collections::HashMap::new()
    });
    let role_ctx = RoleCtx {
        id: role.id.to_string(),
        slug: role.slug.clone(),
        name: role.name.clone(),
        color: role.color.clone(),
        is_system: role.is_system,
        is_default: role.is_default,
        member_count: counts.get(&role.id).copied().unwrap_or(0),
    };

    let all_permissions = state.role.list_all_permissions(&auth_user).await?;
    let perms_ctx: Vec<PermissionCtx> = all_permissions
        .iter()
        .map(|p| PermissionCtx {
            id: p.id.to_string(),
            key: p.key.clone(),
            description: p.description.clone(),
            group_name: p.group_name.clone(),
            min_trust: format!("{:?}", p.min_trust).to_lowercase(),
        })
        .collect();

    let role_permissions = state.role.get_role_permissions(&auth_user, id).await?;
    let role_perm_ids: Vec<String> = role_permissions.iter().map(|p| p.id.to_string()).collect();
    let role_perm_keys: Vec<String> = role_permissions.iter().map(|p| p.key.clone()).collect();

    let mut group_order: Vec<String> = Vec::new();
    let mut group_map: std::collections::HashMap<String, Vec<&PermissionCtx>> =
        std::collections::HashMap::new();
    for p in &perms_ctx {
        if !group_map.contains_key(&p.group_name) {
            group_order.push(p.group_name.clone());
        }
        group_map.entry(p.group_name.clone()).or_default().push(p);
    }

    #[derive(Serialize)]
    struct PermGroupCtx {
        name: String,
        permissions: Vec<PermissionCtx>,
    }
    let permission_groups: Vec<PermGroupCtx> = group_order
        .into_iter()
        .map(|name| PermGroupCtx {
            permissions: group_map[&name].iter().map(|p| (*p).clone()).collect(),
            name,
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("role", &role_ctx);
    ctx.insert("permissions", &perms_ctx);
    ctx.insert("role_permission_ids", &role_perm_ids);
    ctx.insert("role_perm_keys", &role_perm_keys);
    ctx.insert("permission_groups", &permission_groups);

    render_admin(&state, &req_locale, "admin/roles/detail.html", &ctx).await
}

pub async fn permissions(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let all_permissions = state.role.list_all_permissions(&auth_user).await?;

    let mut group_order: Vec<String> = Vec::new();
    let mut group_map: std::collections::HashMap<String, Vec<PermissionCtx>> =
        std::collections::HashMap::new();
    for p in &all_permissions {
        let pctx = PermissionCtx {
            id: p.id.to_string(),
            key: p.key.clone(),
            description: p.description.clone(),
            group_name: p.group_name.clone(),
            min_trust: format!("{:?}", p.min_trust).to_lowercase(),
        };
        if !group_map.contains_key(&p.group_name) {
            group_order.push(p.group_name.clone());
        }
        group_map.entry(p.group_name.clone()).or_default().push(pctx);
    }

    #[derive(Serialize)]
    struct PermGroupCtx {
        name: String,
        permissions: Vec<PermissionCtx>,
    }
    let permission_groups: Vec<PermGroupCtx> = group_order
        .into_iter()
        .map(|name| PermGroupCtx {
            permissions: group_map.remove(&name).unwrap_or_default(),
            name,
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("permission_groups", &permission_groups);

    render_admin(&state, &req_locale, "admin/permissions.html", &ctx).await
}
