use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::EntityTrait;

use crate::app_state::AppState;
use ferum_infrastructure::entities::stored_files;

/// GET /plugins/:slug/assets/*path — serve a plugin UI asset.
///
/// Only files under `{plugins_dir}/{slug}/assets/` are accessible.
/// Path traversal attempts (`..`) in either segment return 404.
pub async fn serve_plugin_asset(
    State(state): State<AppState>,
    Path((slug, asset_path)): Path<(String, String)>,
) -> Response {
    // Reject any traversal attempt in slug or path
    if slug.contains("..") || slug.contains('/') || slug.contains('\\')
        || asset_path.contains("..")
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let file_path = PathBuf::from(&state.plugins_dir)
        .join(&slug)
        .join("assets")
        .join(&asset_path);

    match tokio::fs::read(&file_path).await {
        Ok(data) => {
            let ct = content_type_for_path(&file_path);
            // security_headers middleware adds nosniff/frame-options/CSP to all responses.
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, ct.to_string()),
                    (header::CACHE_CONTROL, "public, max-age=3600".to_string()),
                ],
                data,
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type_for_path(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("js") | Some("mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// GET /files/:key — serve a file stored in the database.
/// This endpoint is only reached when DatabaseStorageService is active (no S3).
pub async fn serve(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Response {
    let result = stored_files::Entity::find_by_id(&key).one(&state.db).await;

    match result {
        Ok(Some(file)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, file.content_type.clone()),
                (
                    header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable".to_string(),
                ),
                (header::CONTENT_DISPOSITION, "attachment".to_string()),
            ],
            file.data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
