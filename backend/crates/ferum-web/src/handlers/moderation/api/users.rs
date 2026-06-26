use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::report::{TempBanRequest, WarnUserRequest};
use crate::view_models::HandlerResult;
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

pub async fn temp_ban(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<TempBanRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    if body.until <= Utc::now() {
        return Err(AppError::unprocessable("Ban expiry must be in the future").into());
    }
    let actor = auth_user.require_auth()?;
    state.moderation.temp_ban(actor, user_id, body.reason, body.until).await?;
    Ok(StatusCode::NO_CONTENT)
}
