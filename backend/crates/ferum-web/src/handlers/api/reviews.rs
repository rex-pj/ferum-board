use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::product::{ReviewRatingResponse, SubmitRatingRequest};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::shared::AppError;
use ferum_domain::models::review_rating::NewReviewRating;

/// Resolve a thread slug to its id, honouring the caller's view permissions.
async fn thread_id_for(
    state: &AppState,
    auth: Option<&AuthUser>,
    slug: &str,
) -> Result<uuid::Uuid, AppError> {
    let thread = state.thread.get_by_slug(auth, slug, None).await?;
    Ok(thread.id)
}

/// POST /api/threads/:slug/rating — attach the caller's structured rating to their review.
pub async fn submit_rating(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<SubmitRatingRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let thread_id = thread_id_for(&state, Some(actor), &slug).await?;

    let rating = NewReviewRating {
        thread_id,
        overall: body.overall,
        durability: body.durability,
        materials: body.materials,
        comfort: body.comfort,
        aesthetics: body.aesthetics,
        value_for_money: body.value_for_money,
        verified_purchase: body.verified_purchase,
    };

    let saved = state.review.submit_rating(actor, thread_id, rating).await?;
    Ok((
        StatusCode::OK,
        Json(DataResponse::new(ReviewRatingResponse::from(saved))),
    ))
}

/// GET /api/threads/:slug/rating — the rating attached to a review thread.
pub async fn get_rating(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let thread_id = thread_id_for(&state, auth_user.as_ref(), &slug).await?;
    let rating = state
        .review
        .get_rating(thread_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(DataResponse::new(ReviewRatingResponse::from(rating))))
}

/// DELETE /api/threads/:slug/rating — remove the caller's own rating.
pub async fn delete_rating(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let thread_id = thread_id_for(&state, Some(actor), &slug).await?;
    state.review.delete_rating(actor, thread_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
