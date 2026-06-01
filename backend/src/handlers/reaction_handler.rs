use axum::extract::{Extension, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::application::shared::AppError;
use crate::middleware::AuthUser;
use crate::view_models::reaction::{counts_to_response, AddReactionRequest};
use crate::view_models::{DataResponse, HandlerResult};

pub async fn add_reaction_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(post_id): Path<Uuid>,
    Json(body): Json<AddReactionRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let kind = body.kind.parse()
        .map_err(|_| AppError::unprocessable("Invalid reaction kind. Use: like, helpful, insightful, funny"))?;

    let counts = state.reaction.add(actor, post_id, kind).await?;
    Ok(Json(DataResponse::new(counts_to_response(counts))))
}

pub async fn remove_reaction_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((post_id, kind_str)): Path<(Uuid, String)>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let kind = kind_str.parse()
        .map_err(|_| AppError::unprocessable("Invalid reaction kind"))?;

    let counts = state.reaction.remove(actor, post_id, kind).await?;
    Ok(Json(DataResponse::new(counts_to_response(counts))))
}
