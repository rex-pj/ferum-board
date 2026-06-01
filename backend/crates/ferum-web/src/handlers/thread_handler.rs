use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::thread::{
    MarkSolvedRequest, MoveThreadRequest, ThreadListQuery, ThreadResponse,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_application::usecases::thread_usecase::CreateThreadCmd;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn validate_image_field(content_type: &str, data: &bytes::Bytes) -> Result<(), AppError> {
    if !matches!(
        content_type,
        "image/jpeg" | "image/png" | "image/webp" | "image/gif"
    ) {
        return Err(AppError::UnprocessableEntity(
            "Unsupported image type. Allowed: JPEG, PNG, WebP, GIF".to_string(),
        ));
    }
    const MAX_SIZE: usize = 2 * 1024 * 1024;
    if data.len() > MAX_SIZE {
        return Err(AppError::UnprocessableEntity(
            "Thumbnail exceeds 2 MB limit".to_string(),
        ));
    }
    if !ferum_application::validators::validate_image_magic(data) {
        return Err(AppError::UnprocessableEntity(
            "File content does not match a supported image format (JPEG, PNG, WebP, GIF)"
                .to_string(),
        ));
    }
    Ok(())
}

// ─── Thread list / get ─────────────────────────────────────────────────────────

/// GET /api/threads — global feed across all visible categories
pub async fn list_threads_by_feed(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ThreadListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let (threads, total) = state
        .thread
        .list_feed(auth_user.as_ref(), page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        threads.into_iter().map(ThreadResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn list_threads_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ThreadListQuery>,
    Path(category_slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let (threads, total) = state
        .thread
        .list_by_category(auth_user.as_ref(), &category_slug, page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        threads.into_iter().map(ThreadResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn get_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let thread = state.thread.get_by_slug(auth_user.as_ref(), &slug).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

// ─── Create ────────────────────────────────────────────────────────────────────

/// POST /api/threads — accepts multipart/form-data.
/// Fields: category_id (text), title (text), content_md (text), thumbnail (file, optional).
pub async fn create_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let mut category_id: Option<Uuid> = None;
    let mut title: Option<String> = None;
    let mut content_md: Option<String> = None;
    let mut thumbnail: Option<(bytes::Bytes, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        match field.name() {
            Some("category_id") => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                category_id = Some(s.parse::<Uuid>().map_err(|_| {
                    AppError::UnprocessableEntity("Invalid category_id".to_string())
                })?);
            }
            Some("title") => {
                title = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?,
                );
            }
            Some("content_md") => {
                content_md = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?,
                );
            }
            Some("thumbnail") => {
                let ct = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                validate_image_field(&ct, &data)?;
                thumbnail = Some((data, ct));
            }
            _ => {}
        }
    }

    let category_id = category_id
        .ok_or_else(|| AppError::UnprocessableEntity("category_id is required".to_string()))?;
    let title =
        title.ok_or_else(|| AppError::UnprocessableEntity("title is required".to_string()))?;
    let content_md = content_md
        .ok_or_else(|| AppError::UnprocessableEntity("content_md is required".to_string()))?;

    if title.len() < 5 || title.len() > 255 {
        return Err(
            AppError::UnprocessableEntity("Title must be 5–255 characters".to_string()).into(),
        );
    }
    if content_md.is_empty() {
        return Err(AppError::UnprocessableEntity("Content is required".to_string()).into());
    }

    let mut thread = state
        .thread
        .create(actor, CreateThreadCmd { category_id, title })
        .await?;

    let post = state
        .post
        .create(
            actor,
            ferum_application::usecases::post_usecase::CreatePostCmd {
                thread_id: thread.id,
                parent_id: None,
                content_md,
            },
        )
        .await?;

    if let Some((data, content_type)) = thumbnail {
        let url = state
            .thread
            .set_thumbnail(actor, thread.id, data, content_type)
            .await?;
        thread.thumbnail_url = Some(url);
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "data": {
                "thread": ThreadResponse::from(thread),
                "first_post": crate::view_models::post::PostResponse::from(post),
            }
        })),
    ))
}

// ─── Update ────────────────────────────────────────────────────────────────────

/// PATCH /api/threads/:id — accepts multipart/form-data.
/// Fields: title (text, optional), thumbnail (file, optional). At least one required.
pub async fn update_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let mut title: Option<String> = None;
    let mut thumbnail: Option<(bytes::Bytes, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        match field.name() {
            Some("title") => {
                title = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?,
                );
            }
            Some("thumbnail") => {
                let ct = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                validate_image_field(&ct, &data)?;
                thumbnail = Some((data, ct));
            }
            _ => {}
        }
    }

    if title.is_none() && thumbnail.is_none() {
        return Err(AppError::UnprocessableEntity(
            "At least one of title or thumbnail must be provided".to_string(),
        )
        .into());
    }

    if let Some(ref t) = title {
        if t.len() < 5 || t.len() > 255 {
            return Err(AppError::UnprocessableEntity(
                "Title must be 5–255 characters".to_string(),
            )
            .into());
        }
    }

    let mut thread = match title {
        Some(t) => state.thread.update_title(actor, id, t).await?,
        None => state.thread.get_by_id(id).await?,
    };

    if let Some((data, content_type)) = thumbnail {
        let url = state
            .thread
            .set_thumbnail(actor, id, data, content_type)
            .await?;
        thread.thumbnail_url = Some(url);
    }

    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

// ─── Other thread actions ──────────────────────────────────────────────────────

pub async fn delete_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.thread.soft_delete(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn pin_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let assigned = get_assigned(&state, actor.id).await?;
    let pin = body.get("pinned").and_then(|v| v.as_bool()).unwrap_or(true);
    let thread = state.thread.pin(actor, id, &assigned, pin).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

pub async fn lock_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let assigned = get_assigned(&state, actor.id).await?;
    let lock = body.get("locked").and_then(|v| v.as_bool()).unwrap_or(true);
    let thread = state.thread.lock(actor, id, &assigned, lock).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

pub async fn move_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<MoveThreadRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let assigned = get_assigned(&state, actor.id).await?;
    let thread = state
        .thread
        .move_to(actor, id, &assigned, body.category_id)
        .await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

pub async fn solve_thread_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<MarkSolvedRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let assigned = get_assigned(&state, actor.id).await?;
    let thread = state
        .thread
        .mark_solved(actor, id, body.best_answer_id, &assigned)
        .await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

// ─── Thumbnail ─────────────────────────────────────────────────────────────────

/// POST /api/threads/:id/thumbnail — upload or replace a thread thumbnail.
/// Accepts multipart/form-data with a single "file" field.
pub async fn upload_thumbnail_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut content_type = "application/octet-stream".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("file") {
            let ct = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
            validate_image_field(&ct, &data)?;
            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;
    let url = state
        .thread
        .set_thumbnail(actor, id, data, content_type)
        .await?;
    Ok(Json(
        serde_json::json!({ "data": { "thumbnail_url": url } }),
    ))
}

/// DELETE /api/threads/:id/thumbnail — remove the manually uploaded thumbnail.
pub async fn delete_thumbnail_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.thread.remove_thumbnail(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Private helpers ───────────────────────────────────────────────────────────

async fn get_assigned(state: &AppState, user_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    state.cat_mod_repo.list_category_ids_for_user(user_id).await
}
