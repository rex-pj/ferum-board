use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::post::PostResponse;
use crate::view_models::{HandlerResult, PagedResponse};

#[derive(serde::Deserialize)]
pub struct PendingQueueQuery {
    pub category_id: Option<uuid::Uuid>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

pub async fn list_pending_posts(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<PendingQueueQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, 100);
    let (posts, total) = state.post.list_pending(actor, q.category_id, page, per_page).await?;
    Ok(Json(PagedResponse::new(
        posts.into_iter().map(PostResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn approve_post(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.post.approve_post(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reject_post(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.post.reject_post(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
