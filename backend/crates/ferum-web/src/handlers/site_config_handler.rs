use axum::extract::{Extension, Multipart, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::constants::{MAX_FAVICON_BYTES, MAX_LOGO_BYTES};
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_infrastructure::storage::cas::{
    cas_key, validate_favicon_content_type, validate_image_content_type,
};

pub async fn get_public_config_handler(
    State(state): State<AppState>,
) -> HandlerResult<impl IntoResponse> {
    let all = state.site_config.get_all().await?;
    let public: HashMap<String, String> = all
        .into_iter()
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "site_name" | "site_tagline" | "logo_url" | "favicon_url" | "primary_color"
            )
        })
        .collect();
    Ok(Json(DataResponse::new(public)))
}

pub async fn get_site_config_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_manage_config(actor)?;
    let config = state.site_config.get_all().await?;
    Ok(Json(DataResponse::new(config)))
}

pub async fn update_site_config_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<HashMap<String, String>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_manage_config(actor)?;

    let allowed_keys = [
        "site_name",
        "site_tagline",
        "logo_url",
        "favicon_url",
        "primary_color",
        "registration_open",
        "keyword_blacklist",
    ];

    let filtered: HashMap<String, String> = body
        .into_iter()
        .filter(|(k, _)| allowed_keys.contains(&k.as_str()))
        .collect();

    state.site_config.set_many(&filtered).await?;
    let config = state.site_config.get_all().await?;
    Ok(Json(DataResponse::new(config)))
}

/// POST /api/admin/config/favicon — upload a new favicon image.
/// Accepts multipart/form-data with a single "file" field.
/// Stores the file via CAS, sets favicon_url in site_config, and releases the old ref.
pub async fn upload_favicon_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_manage_config(actor)?;

    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut content_type = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("file") {
            let ct = field.content_type().unwrap_or("").to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

            if !validate_favicon_content_type(&ct) {
                return Err(
                    AppError::unprocessable("favicon must be ico, svg, png, gif, or jpeg").into(),
                );
            }
            if data.len() > MAX_FAVICON_BYTES {
                return Err(AppError::unprocessable("favicon exceeds 512 KB size limit").into());
            }

            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;

    let key = cas_key("favicons", &data, &content_type);

    if state.stored_files.exists(&key).await? {
        state.stored_files.increment_ref(&key).await?;
    } else {
        state
            .stored_files
            .upsert(
                &key,
                &content_type,
                &data,
                data.len() as i64,
                Some(actor.id),
            )
            .await?;
    }

    // Swap out old favicon and release its ref
    let old_key = state
        .site_config
        .get("favicon_url")
        .await?
        .and_then(|url| url.strip_prefix("/files/").map(str::to_string));

    let favicon_url = format!("/files/{key}");
    state.site_config.set("favicon_url", &favicon_url).await?;

    if let Some(old) = old_key.filter(|k| k != &key) {
        let remaining = state.stored_files.decrement_ref(&old).await?;
        if remaining == 0 {
            let _ = state.stored_files.delete_by_key(&old).await;
        }
    }

    Ok(Json(
        serde_json::json!({ "data": { "favicon_url": favicon_url } }),
    ))
}

/// POST /api/admin/config/logo — upload a new logo image.
/// Accepts multipart/form-data with a single "file" field (JPEG, PNG, WebP, or GIF, max 2 MB).
/// Stores via CAS, sets logo_url in site_config, and releases the old ref if it was an uploaded file.
pub async fn upload_logo_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_manage_config(actor)?;

    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut content_type = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("file") {
            let ct = field.content_type().unwrap_or("").to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

            if !validate_image_content_type(&ct) {
                return Err(AppError::unprocessable("logo must be JPEG, PNG, WebP, or GIF").into());
            }
            if data.len() > MAX_LOGO_BYTES {
                return Err(AppError::unprocessable("logo exceeds 2 MB size limit").into());
            }

            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;

    let key = cas_key("logos", &data, &content_type);

    if state.stored_files.exists(&key).await? {
        state.stored_files.increment_ref(&key).await?;
    } else {
        state
            .stored_files
            .upsert(
                &key,
                &content_type,
                &data,
                data.len() as i64,
                Some(actor.id),
            )
            .await?;
    }

    // Swap out the old logo and release its ref (only if it was an uploaded file)
    let old_key = state
        .site_config
        .get("logo_url")
        .await?
        .and_then(|url| url.strip_prefix("/files/").map(str::to_string));

    let logo_url = format!("/files/{key}");
    state.site_config.set("logo_url", &logo_url).await?;

    if let Some(old) = old_key.filter(|k| k != &key) {
        let remaining = state.stored_files.decrement_ref(&old).await?;
        if remaining == 0 {
            let _ = state.stored_files.delete_by_key(&old).await;
        }
    }

    Ok(Json(
        serde_json::json!({ "data": { "logo_url": logo_url } }),
    ))
}

/// DELETE /api/admin/config/logo — clear the logo and release its stored file ref (if uploaded).
pub async fn delete_logo_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_manage_config(actor)?;

    if let Some(key) = state
        .site_config
        .get("logo_url")
        .await?
        .filter(|url| !url.is_empty())
        .and_then(|url| url.strip_prefix("/files/").map(str::to_string))
    {
        let remaining = state.stored_files.decrement_ref(&key).await?;
        if remaining == 0 {
            let _ = state.stored_files.delete_by_key(&key).await;
        }
    }

    state.site_config.set("logo_url", "").await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/admin/config/favicon — clear the custom favicon and release its stored file ref.
pub async fn delete_favicon_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    PermissionChecker::can_manage_config(actor)?;

    if let Some(key) = state
        .site_config
        .get("favicon_url")
        .await?
        .filter(|url| !url.is_empty())
        .and_then(|url| url.strip_prefix("/files/").map(str::to_string))
    {
        let remaining = state.stored_files.decrement_ref(&key).await?;
        if remaining == 0 {
            let _ = state.stored_files.delete_by_key(&key).await;
        }
    }

    state.site_config.set("favicon_url", "").await?;
    Ok(StatusCode::NO_CONTENT)
}
