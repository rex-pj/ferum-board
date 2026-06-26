use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::app_state::AppState;

pub async fn health(State(_state): State<AppState>) -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
