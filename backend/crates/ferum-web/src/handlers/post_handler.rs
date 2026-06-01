use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::post::{CreatePostRequest, PostListQuery, PostResponse, UpdatePostRequest};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_application::usecases::post_usecase::CreatePostCmd;

pub async fn list_posts_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
    Query(q): Query<PostListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let (posts, total) = state
        .post
        .list_by_thread(auth_user.as_ref(), thread_id, page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        posts.into_iter().map(PostResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn create_post_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
    Json(body): Json<CreatePostRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let post = state
        .post
        .create(
            actor,
            CreatePostCmd {
                thread_id,
                parent_id: body.parent_id,
                content_md: body.content_md,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(PostResponse::from(post))),
    ))
}

pub async fn update_post_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePostRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let post = state.post.edit(actor, id, body.content_md).await?;
    Ok(Json(DataResponse::new(PostResponse::from(post))))
}

pub async fn delete_post_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.post.delete(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
