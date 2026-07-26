use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::report::AuditLogQuery;
use crate::view_models::HandlerResult;
use ferum_application::permission::PermissionChecker;

pub async fn list_audit_log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<AuditLogQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_view_reports(actor, None)?;

    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 30, 100);

    let (logs, total) = state
        .moderation
        .audit_log_repo
        .list(q.actor_id, q.target_type.as_deref(), None, None, None, page, per_page)
        .await?;

    #[derive(Serialize)]
    struct AuditLogResponse {
        id: Uuid,
        actor_id: Option<Uuid>,
        action: String,
        target_type: String,
        target_id: Uuid,
        metadata: Option<serde_json::Value>,
        created_at: DateTime<Utc>,
    }

    Ok(Json(crate::view_models::PagedResponse::new(
        logs.into_iter()
            .map(|l| AuditLogResponse {
                id: l.id,
                actor_id: l.actor_id,
                action: l.action,
                target_type: l.target_type,
                target_id: l.target_id,
                metadata: l.metadata,
                created_at: l.created_at,
            })
            .collect(),
        total,
        page,
        per_page,
    )))
}
