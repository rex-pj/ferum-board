use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::post::{CreatePostRequest, PostListQuery, PostResponse, UpdatePostRequest};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_application::usecases::post_usecase::CreatePostCmd;

pub async fn list_posts(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
    Query(q): Query<PostListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);

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

pub async fn create_post(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(thread_id): Path<Uuid>,
    Json(body): Json<CreatePostRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;

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

pub async fn update_post(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePostRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;

    let post = state.post.edit(actor, id, body.content_md).await?;
    Ok(Json(DataResponse::new(PostResponse::from(post))))
}

pub async fn delete_post(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.post.delete(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
pub struct PreviewMarkdownRequest {
    pub content: String,
}

#[derive(serde::Serialize)]
pub struct PreviewMarkdownResponse {
    pub html: String,
}

pub async fn preview_markdown(
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<PreviewMarkdownRequest>,
) -> HandlerResult<impl IntoResponse> {
    auth_user.require_auth()?;
    if body.content.len() > ferum_application::constants::MAX_POST_CONTENT_BYTES {
        return Err(AppError::UnprocessableEntity(
            "Content exceeds maximum allowed size".to_string(),
        )
        .into());
    }
    let html = ferum_application::validators::markdown::render_and_sanitize(&body.content)
        .unwrap_or_default();
    Ok(Json(PreviewMarkdownResponse { html }))
}
