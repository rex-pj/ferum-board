use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::tag::{TagListQuery, TagResponse};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::shared::AppError;

/// GET /api/tags?q=... — public, no auth required
pub async fn list_tags(
    State(state): State<AppState>,
    Query(q): Query<TagListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let tags = state.tag.list(q.q).await?;
    Ok(Json(DataResponse::new(
        tags.into_iter().map(TagResponse::from).collect::<Vec<_>>(),
    )))
}

/// POST /api/tags — requires TAG_CREATE permission (trust_level >= Member)
pub async fn create_tag(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateTagRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let tags = state
        .tag
        .resolve_or_create(actor, vec![body.name], 1)
        .await?;
    let tag = tags.into_iter().next().ok_or(AppError::NotFound)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(DataResponse::new(TagResponse::from(tag))),
    ))
}

#[derive(serde::Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
}
