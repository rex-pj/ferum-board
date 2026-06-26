use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::webhook::{CreateWebhookRequest, UpdateWebhookRequest, WebhookResponse};
use crate::view_models::{DataResponse, HandlerResult};

pub async fn list_webhooks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let webhooks = state.webhook.list(actor).await?;
    let items: Vec<WebhookResponse> = webhooks.into_iter().map(WebhookResponse::from).collect();
    Ok(Json(DataResponse::new(items)))
}

pub async fn create_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateWebhookRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let webhook = state
        .webhook
        .create(actor, body.url, body.events, body.secret)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(WebhookResponse::from(webhook))),
    ))
}

pub async fn update_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateWebhookRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let webhook = state
        .webhook
        .update(
            actor,
            id,
            body.url,
            body.events,
            body.secret,
            body.is_active,
        )
        .await?;
    Ok(Json(DataResponse::new(WebhookResponse::from(webhook))))
}

pub async fn delete_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.webhook.delete(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
