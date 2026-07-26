use axum::extract::{Extension, Multipart, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::ports::HookContext;

/// Generic inbound entry point for Tier 2 (Script) plugins that need to accept
/// arbitrary client requests — not just react to core hooks. The plugin's JS
/// bundle registers a handler via `__ferum_rpc[action] = function(ctx) {...}`;
/// this route forwards the caller's identity (if any) plus the request body to
/// that handler and returns whatever JSON it produces.
///
/// Deliberately does NOT require auth at this layer: per NF-UX-05, guests can
/// read all public content without logging in, and most plugin actions split
/// into public reads (e.g. viewing poll results, chat history) and
/// member-only writes (voting, posting). The host can't know which is which
/// for an arbitrary plugin action, so `ctx.actor_id` is `null` for anonymous
/// callers and each __ferum_rpc handler decides for itself whether an action
/// needs a logged-in actor — see the `if (!ctx.actor_id) return {...}` guards
/// in the write actions of examples/plugins/{simple-chatbox,community-polls}.
///
/// Unlike before-hooks, a failed dispatch here is a real HTTP error — there is
/// no "fail open" for a request whose entire purpose is to get a response.
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
