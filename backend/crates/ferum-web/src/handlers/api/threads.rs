use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::utils::{validate_upload_image, ImageKind};
use crate::view_models::thread::{
    MarkSolvedRequest, MoveThreadRequest, ThreadListQuery, ThreadResponse,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::constants::MAX_THUMBNAIL_BYTES;
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_application::usecases::thread_usecase::CreateThreadCmd;
use ferum_domain::repositories::thread_repository::parse_feed_query;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Thumbnail rules = the shared image rules, under the thumbnail limit.
///
/// `ThreadUseCase::set_thumbnail` is the authority; this repeats the check only
/// because create/update reach it *after* the thread is written, so a late
/// failure would need a compensating delete. A guard on ordering, not a second
/// opinion — never give it its own size constant.
fn validate_image_field(content_type: &str, data: &bytes::Bytes) -> Result<(), AppError> {
    validate_upload_image(content_type, data, MAX_THUMBNAIL_BYTES, ImageKind::THUMBNAIL)
}

// ─── Thread list / get ─────────────────────────────────────────────────────────

/// GET /api/threads — global feed across all visible categories (optionally filtered by ?tag=slug)
pub async fn list_feed(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ThreadListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, state.thread.max_page_size().await)?;

    // No redirect here, unlike the HTML pages: an API client following a 301 is
    // best case a wasted round trip and worst case a broken integration, and
    // there is no search index to keep clean. Legacy spellings simply keep working.
    let (sort, feed_filter, _legacy) = parse_feed_query(q.sort.as_deref(), q.filter.as_deref());
    let (threads, total) = if let Some(tag_slug) = &q.tag {
        state
            .thread
            .list_by_tag(auth_user.as_ref(), tag_slug, sort, feed_filter, page, per_page)
            .await?
    } else {
        state
            .thread
            .list_feed(auth_user.as_ref(), sort, feed_filter, page, per_page)
            .await?
    };

    Ok(Json(PagedResponse::new(
        threads.into_iter().map(ThreadResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn list_threads(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ThreadListQuery>,
    Path(category_slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, state.thread.max_page_size().await)?;

    let (sort, feed_filter, _legacy) = parse_feed_query(q.sort.as_deref(), q.filter.as_deref());
    let (threads, total) = state
        .thread
        .list_by_category(auth_user.as_ref(), &category_slug, sort, feed_filter, page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        threads.into_iter().map(ThreadResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn get_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let thread = state.thread.get_by_slug(auth_user.as_ref(), &slug, None).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

// ─── Create ────────────────────────────────────────────────────────────────────

/// POST /api/threads — accepts multipart/form-data.
/// Fields: category_id (text), title (text), content_md (text), thumbnail (file, optional).
pub async fn create_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    let mut category_id: Option<Uuid> = None;
    let mut title: Option<String> = None;
    let mut content_md: Option<String> = None;
    let mut thumbnail: Option<(bytes::Bytes, String)> = None;
    let mut tag_names: Vec<String> = Vec::new();
    let mut product_id: Option<Uuid> = None;
    // Structured rating — present only for product reviews, submitted in the same
    // request so a review and its rating are created as one operation.
    let mut overall: Option<i16> = None;
    let mut durability: Option<i16> = None;
    let mut materials: Option<i16> = None;
    let mut comfort: Option<i16> = None;
    let mut aesthetics: Option<i16> = None;
    let mut value_for_money: Option<i16> = None;
    let mut verified_purchase = false;

    // Parse a multipart text field into an optional i16 score.
    async fn score_field(field: axum::extract::multipart::Field<'_>) -> Result<Option<i16>, AppError> {
        let s = field
            .text()
            .await
            .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
        let s = s.trim();
        if s.is_empty() {
            return Ok(None);
        }
        s.parse::<i16>()
            .map(Some)
            .map_err(|_| AppError::UnprocessableEntity("Invalid rating value".to_string()))
    }

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
            Some("tags") => {
                let raw = field
                    .text()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                // Accept either repeated fields or comma-separated values
                for name in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if tag_names.len() < 5 {
                        tag_names.push(name.to_string());
                    }
                }
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
            Some("product_id") => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                let s = s.trim();
                if !s.is_empty() {
                    product_id = Some(s.parse::<Uuid>().map_err(|_| {
                        AppError::UnprocessableEntity("Invalid product_id".to_string())
                    })?);
                }
            }
            Some("overall") => overall = score_field(field).await?,
            Some("durability") => durability = score_field(field).await?,
            Some("materials") => materials = score_field(field).await?,
            Some("comfort") => comfort = score_field(field).await?,
            Some("aesthetics") => aesthetics = score_field(field).await?,
            Some("value_for_money") => value_for_money = score_field(field).await?,
            Some("verified_purchase") => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                verified_purchase = matches!(s.trim(), "true" | "1" | "on");
            }
            _ => {}
        }
    }

    // A product review always lands in the canonical Reviews category (resolved
    // server-side) — the submitter never picks a forum category, and every
    // product's reviews stay grouped together. Only plain threads need one.
    let category_id = if product_id.is_some() {
        state.category.ensure_reviews_category().await?.id
    } else {
        category_id
            .ok_or_else(|| AppError::UnprocessableEntity("category_id is required".to_string()))?
    };
    let title =
        title.ok_or_else(|| AppError::UnprocessableEntity("title is required".to_string()))?;
    let content_md = content_md
        .ok_or_else(|| AppError::UnprocessableEntity("content_md is required".to_string()))?;

    // Title length belongs to `ThreadUseCase::create`, which checks before any
    // write — do NOT re-check it here. A copy once counted `title.len()` in
    // bytes against character bounds, refusing Vietnamese titles the validator
    // accepts.
    //
    // Content is different: `PostUseCase::create` sees it only after the thread
    // row exists, so an empty body there would cost a compensating delete.
    if content_md.trim().is_empty() {
        return Err(AppError::invalid("post_content_empty").into());
    }
    if thumbnail.is_some() {
        PermissionChecker::can_upload(actor)?;
    }

    // Enforce the review invariant up front (before any write): a product review
    // must carry a valid overall score, and every provided score is 1–5. Failing
    // here means nothing is created — no orphan thread to roll back.
    if product_id.is_some() {
        let o = overall.ok_or_else(|| {
            AppError::UnprocessableEntity("A star rating is required for a product review.".into())
        })?;
        for s in [Some(o), durability, materials, comfort, aesthetics, value_for_money]
            .into_iter()
            .flatten()
        {
            if !(1..=5).contains(&s) {
                return Err(AppError::UnprocessableEntity("Ratings must be between 1 and 5.".into()).into());
            }
        }
    }

    let mut thread = state
        .thread
        .create(actor, CreateThreadCmd { category_id, title, content_md: content_md.clone(), tag_names, product_id })
        .await?;

    let post = match state
        .post
        .create(
            actor,
            ferum_application::usecases::post_usecase::CreatePostCmd {
                thread_id: thread.id,
                parent_id: None,
                content_md,
            },
        )
        .await
    {
        Ok(p) => p,
        Err(e) => {
            // Compensate: mark the thread as deleted so it does not appear in feeds
            // with zero posts. Best-effort — we still return the original error.
            let _ = state.thread.soft_delete(actor, thread.id).await;
            return Err(e.into());
        }
    };

    // Same operation: attach the OP's structured rating. Pre-validated above, so a
    // failure here is a rare DB error — roll the whole review back rather than
    // leave a rated-review thread without its rating.
    if let (Some(_), Some(o)) = (product_id, overall) {
        let rating = ferum_domain::models::review_rating::NewReviewRating {
            thread_id: thread.id,
            overall: o,
            durability,
            materials,
            comfort,
            aesthetics,
            value_for_money,
            verified_purchase,
        };
        if let Err(e) = state.review.submit_rating(actor, thread.id, rating).await {
            let _ = state.thread.soft_delete(actor, thread.id).await;
            return Err(e.into());
        }
    }

    if let Some((data, content_type)) = thumbnail {
        let url = state
            .thread
            // No crop: this arrives inside the compose form, which has no
            // cropper. The dedicated /thumbnail endpoint is the one that does.
            .set_thumbnail(actor, thread.id, data, content_type, None)
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
/// Fields: title (text, optional), content_md (text, optional), tags (text, optional,
/// comma-separated), thumbnail (file, optional). At least one field is required.
pub async fn update_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let id = state.thread.id_for_slug(&slug).await?;

    let mut title: Option<String> = None;
    let mut content_md: Option<String> = None;
    let mut tag_names: Option<Vec<String>> = None;
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
            Some("content_md") => {
                content_md = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?,
                );
            }
            Some("tags") => {
                let raw = field
                    .text()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                tag_names = Some(
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .take(5)
                        .collect(),
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

    if title.is_none() && content_md.is_none() && tag_names.is_none() && thumbnail.is_none() {
        return Err(AppError::UnprocessableEntity(
            "At least one of title, content_md, tags, or thumbnail must be provided".to_string(),
        )
        .into());
    }

    // No title-length check here either — `ThreadUseCase::update_title` owns it
    // and counts characters, not bytes. See `create_thread`.
    if thumbnail.is_some() {
        PermissionChecker::can_upload(actor)?;
    }

    let mut thread = match title {
        Some(t) => state.thread.update_title(actor, id, t).await?,
        None => state.thread.get_by_id(id).await?,
    };

    if let Some(content_md) = content_md {
        let (first_post, _) = state.post.list_by_thread(Some(actor), id, 1, 1).await?;
        if let Some(post) = first_post.into_iter().next() {
            state.post.edit(actor, post.id, content_md).await?;
        }
    }

    if let Some(tag_names) = tag_names {
        state.thread.update_tags(actor, id, tag_names).await?;
        thread = state.thread.get_by_id(id).await?;
    }

    if let Some((data, content_type)) = thumbnail {
        let url = state
            .thread
            .set_thumbnail(actor, id, data, content_type, None)
            .await?;
        thread.thumbnail_url = Some(url);
    }

    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

// ─── Other thread actions ──────────────────────────────────────────────────────

pub async fn delete_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.thread.delete_by_slug(actor, &slug).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn pin_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let pin = body.get("pinned").and_then(|v| v.as_bool()).unwrap_or(true);
    let id = state.thread.id_for_slug(&slug).await?;
    let thread = state.thread.pin(actor, id, pin).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

pub async fn lock_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let lock = body.get("locked").and_then(|v| v.as_bool()).unwrap_or(true);
    let id = state.thread.id_for_slug(&slug).await?;
    let thread = state.thread.lock(actor, id, lock).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

pub async fn move_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<MoveThreadRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let id = state.thread.id_for_slug(&slug).await?;
    let thread = state.thread.move_to(actor, id, body.category_id).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

pub async fn solve_thread(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<MarkSolvedRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let id = state.thread.id_for_slug(&slug).await?;
    let thread = state.thread.mark_solved(actor, id, body.best_answer_id).await?;
    Ok(Json(DataResponse::new(ThreadResponse::from(thread))))
}

// ─── Thumbnail ─────────────────────────────────────────────────────────────────

/// POST /api/threads/:slug/thumbnail — upload or replace a thread thumbnail.
/// Accepts multipart/form-data with a single "file" field.
pub async fn upload_thumbnail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let id = state.thread.id_for_slug(&slug).await?;

    // No pre-check here: unlike create/update this endpoint's very next call is
    // `set_thumbnail`, so the use case rejects a bad file before anything is
    // written and a second copy of the rules would buy nothing.
    let (data, content_type, crop) =
        crate::utils::read_image_field_with_crop(&mut multipart, "file").await?;
    let url = state
        .thread
        .set_thumbnail(actor, id, data, content_type, crop)
        .await?;
    Ok(Json(
        serde_json::json!({ "data": { "thumbnail_url": url } }),
    ))
}

/// DELETE /api/threads/:slug/thumbnail — remove the manually uploaded thumbnail.
pub async fn delete_thumbnail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let id = state.thread.id_for_slug(&slug).await?;
    state.thread.remove_thumbnail(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
