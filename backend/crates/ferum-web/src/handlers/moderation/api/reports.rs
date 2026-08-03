use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::report::{
    CreateReportRequest, ReportListQuery, ReportResponse, ResolveReportRequest,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_application::usecases::moderation_usecase::CreateReportCmd;
use ferum_domain::models::report::ReportStatus;

pub async fn create_report(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateReportRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    if body.post_id.is_none() && body.thread_id.is_none() {
        return Err(AppError::invalid("report_target_required").into());
    }
    let actor = auth_user.require_auth()?;
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

/// GET /api/reports/mine — reports the caller filed themselves, so they can
/// track resolution status without needing moderator access.
pub async fn list_my_reports(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ReportListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, 100)?;

    let (reports, total) = state.moderation.list_my_reports(actor, page, per_page).await?;
    Ok(Json(PagedResponse::new(
        reports.into_iter().map(ReportResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn list_reports(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ReportListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, 100)?;
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

pub async fn resolve_report(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveReportRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;
    let status = match body.status.as_str() {
        "resolved"  => ReportStatus::Resolved,
        "dismissed" => ReportStatus::Dismissed,
        _           => unreachable!("validated above"),
    };
    state
        .moderation
        .resolve_report(actor, id, status, body.moderator_notes)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
