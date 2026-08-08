use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::notification::{NotificationResponse, UnreadCountResponse};
use crate::view_models::post::PostListQuery;
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};

pub async fn list_notifications(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<PostListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, ferum_application::constants::MAX_LIST_PAGE_SIZE)?;

    let (notifs, total) = state.notification.inbox(actor, page, per_page).await?;
    Ok(Json(PagedResponse::new(
        notifs.into_iter().map(NotificationResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn unread_count(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let count = state.notification.unread_count(actor).await?;
    Ok(Json(DataResponse::new(UnreadCountResponse { count })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.notification.mark_read(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.notification.mark_all_read(actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Server-Sent Events stream for real-time notifications.
/// Requires authentication; each event is a JSON object on the "notification" channel.
pub async fn sse_stream(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let rx = state.broadcaster.subscribe(actor.id);

    let stream = UnboundedReceiverStream::new(rx).map(|data| -> Result<Event, Infallible> {
        Ok(Event::default().event("notification").data(data))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(25)).text("")))
}
