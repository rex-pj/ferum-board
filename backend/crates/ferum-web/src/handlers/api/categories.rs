use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::category::CategoryResponse;
use crate::view_models::{DataResponse, HandlerResult};

pub async fn list_categories(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let categories = state.category.list_visible(auth_user.as_ref()).await?;
    let data: Vec<CategoryResponse> = categories.into_iter().map(Into::into).collect();
    Ok(Json(DataResponse::new(data)))
}

pub async fn get_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let category = state
        .category
        .get_by_slug(auth_user.as_ref(), &slug)
        .await?;
    Ok(Json(DataResponse::new(CategoryResponse::from(category))))
}

// ─── Watch / Mute ─────────────────────────────────────────────────────────────

pub async fn watch_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.user_repo.watch_category(actor.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unwatch_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.user_repo.unwatch_category(actor.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn mute_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.user_repo.mute_category(actor.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unmute_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.user_repo.unmute_category(actor.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_watch_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let watched = state.user_repo.get_watched_categories(actor.id).await?;
    let muted = state.user_repo.get_muted_categories(actor.id).await?;
    Ok(Json(DataResponse::new(serde_json::json!({
        "watched": watched.contains(&id),
        "muted": muted.contains(&id),
    }))))
}
