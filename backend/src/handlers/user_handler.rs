use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::application::shared::AppError;
use crate::middleware::AuthUser;
use crate::view_models::auth::UserResponse;
use crate::view_models::thread::{ThreadListQuery, ThreadResponse};
use crate::view_models::user::UserSummaryResponse;
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};

pub async fn get_public_profile_handler(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let user = state
        .auth
        .users
        .find_by_username(&username)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(DataResponse::new(UserSummaryResponse::from(user))))
}

pub async fn list_user_threads_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(username): Path<String>,
    Query(q): Query<ThreadListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let user = state
        .auth
        .users
        .find_by_username(&username)
        .await?
        .ok_or(AppError::NotFound)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let (threads, total) = state
        .thread
        .list_by_author(auth_user.as_ref(), user.id, page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        threads.into_iter().map(ThreadResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn get_me_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let auth = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let user = state
        .auth
        .users
        .find_by_id(auth.id)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(DataResponse::new(UserResponse::from(user))))
}
