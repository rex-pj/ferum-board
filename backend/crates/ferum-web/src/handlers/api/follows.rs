use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::follow::{FollowListQuery, FollowResponse, FollowStatusResponse};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};

pub async fn follow_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.follow.follow(actor, user_id).await?;
    let status = state.follow.get_status(Some(actor.id), user_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(FollowStatusResponse {
            following: status.following,
            follower_count: status.follower_count,
            following_count: status.following_count,
        })),
    ))
}

pub async fn unfollow_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.follow.unfollow(actor, user_id).await?;
    let status = state.follow.get_status(Some(actor.id), user_id).await?;
    Ok(Json(DataResponse::new(FollowStatusResponse {
        following: status.following,
        follower_count: status.follower_count,
        following_count: status.following_count,
    })))
}

pub async fn get_follow_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let viewer_id = auth_user.as_ref().map(|u| u.id);
    let status = state.follow.get_status(viewer_id, user_id).await?;
    Ok(Json(DataResponse::new(FollowStatusResponse {
        following: status.following,
        follower_count: status.follower_count,
        following_count: status.following_count,
    })))
}

pub async fn list_followers(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(query): Query<FollowListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let (page, per_page) = crate::utils::paginate(query.page, query.per_page, 20, 50);

    let (pairs, total) = state.follow.list_followers(user_id, page, per_page).await?;
    let items: Vec<FollowResponse> = pairs
        .into_iter()
        .map(|(f, u)| FollowResponse::from_pair(f, u))
        .collect();

    Ok(Json(PagedResponse::new(items, total, page, per_page)))
}

pub async fn list_following(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(query): Query<FollowListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let (page, per_page) = crate::utils::paginate(query.page, query.per_page, 20, 50);

    let (pairs, total) = state.follow.list_following(user_id, page, per_page).await?;
    let items: Vec<FollowResponse> = pairs
        .into_iter()
        .map(|(f, u)| FollowResponse::from_pair(f, u))
        .collect();

    Ok(Json(PagedResponse::new(items, total, page, per_page)))
}
