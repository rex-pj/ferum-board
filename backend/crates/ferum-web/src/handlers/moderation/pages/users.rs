use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::moderation::ModPageQuery;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AdminUserRowCtx, CurrentUserCtx, PaginationCtx, RoleCtx};

use super::super::require_moderator;

pub async fn users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = crate::utils::page_number(q.page).map_err(|_| PageError::NotFound)?;
    let per_page = 20u64;
    let search = q.q.clone();

    let (users, total) = state
        .moderation
        .search_users(&auth_user, search.as_deref(), page, per_page)
        .await?;

    let user_ids: Vec<uuid::Uuid> = users.iter().map(|u| u.id).collect();
    let assignments = state
        .role
        .list_role_assignments_for_users(&user_ids)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load role assignments for mod user list");
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
    ctx.insert("q", &search.unwrap_or_default());
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));

    render_admin(&state, &req_locale, "mod/users.html", ctx).await
}
