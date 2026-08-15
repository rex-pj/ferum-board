use axum::extract::{Extension, Multipart, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::ports::HookContext;

/// Inbound entry point for Script plugins accepting arbitrary client requests.
/// The bundle registers `__ferum_rpc[action]`; this forwards caller identity and
/// body, and returns whatever JSON the handler produces.
///
/// **Deliberately unauthenticated** — the host cannot know which of a plugin's
/// actions are public reads, so `ctx.actor_id` is null for guests and **every
/// write action must check it itself** (see the guards in simple-chatbox).
///
/// Unlike before-hooks this does NOT fail open: a bad dispatch is a real error.
pub async fn invoke(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((slug, action)): Path<(String, String)>,
    body: Option<Json<serde_json::Value>>,
) -> HandlerResult<impl IntoResponse> {
    let ctx = HookContext {
        hook_name: format!("rpc:{action}"),
        actor_id: auth_user.as_ref().map(|u| u.id),
        actor_trust_level: auth_user
            .as_ref()
            .map(|u| format!("{:?}", u.trust_level).to_lowercase())
            .unwrap_or_else(|| "guest".to_string()),
        payload: serde_json::json!({
            "actor_username": auth_user.as_ref().map(|u| u.username.clone()),
            "actor_display_name": auth_user.as_ref().and_then(|u| u.display_name.clone()),
            "body": body.map(|Json(v)| v).unwrap_or(serde_json::Value::Null),
        }),
    };

    let result = state.plugin_rpc.dispatch_rpc(&slug, &action, &ctx).await?;
    Ok(Json(DataResponse::new(result)))
}

#[derive(Serialize)]
pub struct UploadMediaResponse {
    pub url: String,
}

/// Multipart upload for plugin-owned media (e.g. a story/gallery image) — kept
/// separate from the JSON RPC endpoint above because a multi-MB file, base64'd
/// into JSON and parsed inside the single-threaded JS sandbox, would risk the
/// hook timeout on anything but tiny images. Reuses the same CAS storage
/// backend as avatar/cover uploads, namespaced under `plugin_{slug}` so a
/// plugin can never collide with another plugin's — or core's — files.
pub async fn upload_media(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    // Validation stays in the use case: it owns the per-plugin media limits and
    // the `granted_capabilities.media` check.
    let (data, content_type) = crate::utils::read_file_field(&mut multipart, "file").await?;
    let url = state.plugin.upload_media(actor, &slug, data, content_type).await?;
    Ok(Json(DataResponse::new(UploadMediaResponse { url })))
}
