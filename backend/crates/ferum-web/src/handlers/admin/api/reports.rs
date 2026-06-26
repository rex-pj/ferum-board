use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::report::{ReportListQuery, ReportResponse};
use crate::view_models::{HandlerResult, PagedResponse};
use ferum_domain::models::report::ReportStatus;

pub async fn list_reports(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ReportListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).min(100);

    let status = q.status.as_deref().and_then(|s| match s {
        "pending" => Some(ReportStatus::Pending),
        "resolved" => Some(ReportStatus::Resolved),
        "dismissed" => Some(ReportStatus::Dismissed),
        _ => None,
    });

    let (reports, total) = state
        .moderation
        .list_all_reports(actor, status, q.target_type.as_deref(), q.q.as_deref(), page, per_page)
        .await?;
    Ok(Json(PagedResponse::new(
        reports.into_iter().map(ReportResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

#[derive(serde::Deserialize)]
pub struct AuditLogQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub actor_id: Option<Uuid>,
    pub target_type: Option<String>,
}

pub async fn list_audit_log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<AuditLogQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(30);

    let (logs, total) = state
        .admin
        .list_audit_log(actor, q.actor_id, q.target_type.as_deref(), None, None, None, page, per_page)
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

    Ok(Json(PagedResponse::new(
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
