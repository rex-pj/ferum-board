use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::bookmark::{BookmarkListQuery, BookmarkResponse, BookmarkStatusResponse};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;

pub async fn list_bookmarks_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(query): Query<BookmarkListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);

    let (pairs, total) = state.bookmark.list(actor, page, per_page).await?;
    let items: Vec<BookmarkResponse> = pairs
        .into_iter()
        .map(|(b, t)| BookmarkResponse::from_pair(b, t))
        .collect();

    Ok(Json(PagedResponse::new(items, total, page, per_page)))
}

pub async fn get_bookmark_status_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let bookmarked = state.bookmark.is_bookmarked(actor.id, thread_id).await?;
    Ok(Json(DataResponse::new(BookmarkStatusResponse {
        bookmarked,
    })))
}

pub async fn add_bookmark_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let bookmarked = state.bookmark.add(actor, thread_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(BookmarkStatusResponse { bookmarked })),
    ))
}

pub async fn remove_bookmark_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let bookmarked = state.bookmark.remove(actor, thread_id).await?;
    Ok(Json(DataResponse::new(BookmarkStatusResponse {
        bookmarked,
    })))
}
