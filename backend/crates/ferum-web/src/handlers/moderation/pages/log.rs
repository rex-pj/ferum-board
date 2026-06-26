use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AuditLogCtx, CurrentUserCtx, PaginationCtx};

use super::super::{require_moderator, ModPageQuery};

pub async fn log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;
    let actor_id_filter = q.actor_id.clone().unwrap_or_default();
    let target_type_filter = q.target_type.clone().unwrap_or_default();

    let actor_uuid = if actor_id_filter.is_empty() {
        None
    } else {
        actor_id_filter.parse::<uuid::Uuid>().ok()
    };
    let target_type_param = if target_type_filter.is_empty() {
        None
    } else {
        Some(target_type_filter.as_str())
    };

    let (logs, total) = state
        .moderation
        .list_audit_log(&auth_user, actor_uuid, target_type_param, None, page, per_page)
        .await?;

    let logs_ctx: Vec<AuditLogCtx> = logs
        .into_iter()
        .map(|l| AuditLogCtx {
            id: l.id.to_string(),
            actor_id: l.actor_id.map(|id| id.to_string()).unwrap_or_default(),
            actor_username: None,
            action: l.action,
            target_type: l.target_type,
            target_id: l.target_id.to_string(),
            created_at: l.created_at.to_rfc3339(),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("logs", &logs_ctx);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("filter", &actor_id_filter);
    ctx.insert("target_type", &target_type_filter);

    render_admin(&state, "mod/log.html", &ctx).await
}
