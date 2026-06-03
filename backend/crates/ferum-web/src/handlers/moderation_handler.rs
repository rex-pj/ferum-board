use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::report::{
    AuditLogQuery, CreateReportRequest, ReportListQuery, ReportResponse, ResolveReportRequest,
    TempBanRequest, WarnUserRequest,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_application::usecases::moderation_usecase::CreateReportCmd;
use ferum_domain::models::report::ReportStatus;

pub async fn create_report_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateReportRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let report = state
        .moderation
        .create_report(
            actor,
            CreateReportCmd {
                post_id: body.post_id,
                thread_id: body.thread_id,
                reason: body.reason,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(ReportResponse::from(report))),
    ))
}

pub async fn list_mod_reports_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ReportListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);
    let target_type = q.target_type.as_deref();

    let (reports, total) = state
        .moderation
        .list_pending_reports(actor, target_type, page, per_page)
        .await?;
    Ok(Json(PagedResponse::new(
        reports.into_iter().map(ReportResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn resolve_report_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveReportRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let status = match body.status.as_str() {
        "resolved" => ReportStatus::Resolved,
        "dismissed" => ReportStatus::Dismissed,
        _ => {
            return Err(AppError::unprocessable("status must be 'resolved' or 'dismissed'").into())
        }
    };
    state
        .moderation
        .resolve_report(actor, id, status, body.moderator_notes)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn warn_user_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<WarnUserRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state
        .moderation
        .warn_user(actor, user_id, body.reason)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn temp_ban_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<TempBanRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state
        .moderation
        .temp_ban(actor, user_id, body.reason, body.until)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_mod_audit_log_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<AuditLogQuery>,
) -> HandlerResult<impl IntoResponse> {
    use chrono::{DateTime, Utc};
    use ferum_application::permission::PermissionChecker;
    use serde::Serialize;
    use uuid::Uuid;

    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_view_reports(actor, None)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(30);

    let (logs, total) = state
        .moderation
        .audit_log_repo
        .list(q.actor_id, q.target_type.as_deref(), page, per_page)
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
