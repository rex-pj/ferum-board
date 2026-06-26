use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::app_state::AppState;

/// Reload Tera templates from disk without restarting the server.
/// Only compiled in debug builds — not present in release binaries.
pub async fn reload_templates(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match state.tera.reload_themes().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true, "message": "Templates reloaded" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "message": e.to_string() })),
        ),
    }
}
