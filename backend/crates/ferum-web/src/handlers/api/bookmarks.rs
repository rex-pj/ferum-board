use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::bookmark::{BookmarkListQuery, BookmarkResponse, BookmarkStatusResponse};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};

pub async fn list_bookmarks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(query): Query<BookmarkListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let (page, per_page) = crate::utils::paginate(query.page, query.per_page, 20, ferum_application::constants::MAX_LIST_PAGE_SIZE)?;

    let (pairs, total) = state.bookmark.list(actor, page, per_page).await?;
    let items: Vec<BookmarkResponse> = pairs
        .into_iter()
        .map(|(b, t)| BookmarkResponse::from_pair(b, t))
        .collect();

    Ok(Json(PagedResponse::new(items, total, page, per_page)))
}

pub async fn get_bookmark_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let thread_id = state.thread.id_for_slug(&slug).await?;
    let bookmarked = state.bookmark.is_bookmarked(actor.id, thread_id).await?;
    Ok(Json(DataResponse::new(BookmarkStatusResponse {
        bookmarked,
    })))
}

pub async fn add_bookmark(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let thread_id = state.thread.id_for_slug(&slug).await?;
    let bookmarked = state.bookmark.add(actor, thread_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(BookmarkStatusResponse { bookmarked })),
    ))
}

pub async fn remove_bookmark(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let thread_id = state.thread.id_for_slug(&slug).await?;
    let bookmarked = state.bookmark.remove(actor, thread_id).await?;
    Ok(Json(DataResponse::new(BookmarkStatusResponse {
        bookmarked,
    })))
}
