use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::lookup::{LookupOption, LookupQuery};
use crate::view_models::report::{TempBanRequest, WarnUserRequest};
use crate::view_models::{HandlerResult, PagedResponse};
use ferum_application::shared::AppError;

pub async fn warn_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<WarnUserRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;
    state.moderation.warn_user(actor, user_id, body.reason).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn lookup_users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<LookupQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, ferum_application::constants::MAX_LIST_PAGE_SIZE)?;
    let search = q.q.as_deref().filter(|s| !s.is_empty());
    let (users, total) = state.moderation.search_users(actor, search, page, per_page).await?;
    let data: Vec<LookupOption> = users.into_iter().map(LookupOption::from).collect();
    Ok(Json(PagedResponse::new(data, total, page, per_page)))
}

pub async fn temp_ban(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<TempBanRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    if body.until <= Utc::now() {
        return Err(AppError::invalid("ban_expiry_must_be_future").into());
    }
    let actor = auth_user.require_auth()?;
    state.moderation.temp_ban(actor, user_id, body.reason, body.until).await?;
    Ok(StatusCode::NO_CONTENT)
}
