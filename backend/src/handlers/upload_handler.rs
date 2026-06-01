use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::EntityTrait;

use crate::app_state::AppState;
use crate::entities::stored_files;

/// GET /files/:key — serve a file stored in the database.
/// This endpoint is only reached when DatabaseStorageService is active (no S3).
pub async fn serve_upload_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Response {
    let result = stored_files::Entity::find_by_id(&key)
        .one(&state.db)
        .await;

    match result {
        Ok(Some(file)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, file.content_type.clone()),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
                (header::CONTENT_DISPOSITION, "attachment".to_string()),
            ],
            file.data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
