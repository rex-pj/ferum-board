use axum::extract::{Extension, Multipart, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::constants::{MAX_FAVICON_BYTES, MAX_LOGO_BYTES};
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_application::storage_utils::{
    cas_key, validate_favicon_content_type, validate_image_content_type,
};
use ferum_application::validators::{validate_favicon_magic, validate_image_magic};

/// Keys readable/writable via the config API (used by both `get_config` and
/// `update_config`, so the two can never drift out of sync). SMTP credentials
/// are intentionally omitted — the mail transport is built once at startup
/// from env vars and does not hot-reload from site_config; see CLAUDE.md.
const CONFIG_READABLE_KEYS: &[&str] = &[
    "site_name",
    "site_tagline",
    "site_slogan",
    "logo_url",
    "favicon_url",
    "primary_color",
    "registration_open",
    "keyword_blacklist",
    "auth_rate_limit_per_min",
    "public_write_rate_limit_per_min",
    "account_lockout_attempts",
    "account_lockout_duration_minutes",
    "post_approval_enabled",
    "post_approval_min_trust",
    "post_edit_window_hours",
    "forum_index_threads_per_category",
    "max_posts_per_page",
    "max_threads_per_page",
];

pub async fn get_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_manage_config(actor)?;
    let all = state.site_config.get_all().await?;
    // Return only the whitelisted keys so internal/infra keys never leak via this endpoint.
    let filtered: std::collections::HashMap<String, String> = all
        .into_iter()
        .filter(|(k, _)| CONFIG_READABLE_KEYS.contains(&k.as_str()))
        .collect();
    Ok(Json(DataResponse::new(filtered)))
}

pub async fn update_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<HashMap<String, String>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_manage_config(actor)?;

    let filtered: HashMap<String, String> = body
        .into_iter()
        .filter(|(k, _)| CONFIG_READABLE_KEYS.contains(&k.as_str()))
        .collect();

    state.site_config.set_many(&filtered).await?;
    {
        let mut cache = state.site_config_cache.write().await;
        for (k, v) in &filtered {
            cache.insert(k.clone(), v.clone());
        }
    }
    let config = state.site_config.get_all().await?;
    Ok(Json(DataResponse::new(config)))
}

/// POST /api/admin/config/favicon — upload a new favicon image.
pub async fn upload_favicon(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
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

            if !validate_favicon_content_type(&ct) || !validate_favicon_magic(&data) {
                return Err(
                    AppError::invalid("favicon_invalid_type").into(),
                );
            }
            if data.len() > MAX_FAVICON_BYTES {
                return Err(AppError::invalid_with("favicon_too_large", [("limit_kb", (MAX_FAVICON_BYTES / 1024).into())]).into());
            }

            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;

    let key = cas_key("favicons", &data, &content_type);

    state
        .stored_files
        .upsert_and_ref(&key, &content_type, &data, data.len() as i64, Some(actor.id))
        .await?;

    let old_key = state
        .site_config
        .get("favicon_url")
        .await?
        .and_then(|url| url.strip_prefix("/files/").map(str::to_string));

    let favicon_url = format!("/files/{key}");
    state.site_config.set("favicon_url", &favicon_url).await?;
    state.site_config_cache.write().await.insert("favicon_url".to_string(), favicon_url.clone());

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

/// DELETE /api/admin/config/favicon — clear the custom favicon.
pub async fn delete_favicon(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
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
    state.site_config_cache.write().await.insert("favicon_url".to_string(), String::new());
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/admin/config/logo — upload a new logo image.
pub async fn upload_logo(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
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

            if !validate_image_content_type(&ct) || !validate_image_magic(&data) {
                return Err(AppError::invalid("logo_invalid_type").into());
            }
            if data.len() > MAX_LOGO_BYTES {
                return Err(AppError::invalid_with("logo_too_large", [("limit_mb", (MAX_LOGO_BYTES / (1024 * 1024)).into())]).into());
            }

            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;

    let key = cas_key("logos", &data, &content_type);

    state
        .stored_files
        .upsert_and_ref(&key, &content_type, &data, data.len() as i64, Some(actor.id))
        .await?;

    let old_key = state
        .site_config
        .get("logo_url")
        .await?
        .and_then(|url| url.strip_prefix("/files/").map(str::to_string));

    let logo_url = format!("/files/{key}");
    state.site_config.set("logo_url", &logo_url).await?;
    state.site_config_cache.write().await.insert("logo_url".to_string(), logo_url.clone());

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

/// DELETE /api/admin/config/logo — clear the logo.
pub async fn delete_logo(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
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
    state.site_config_cache.write().await.insert("logo_url".to_string(), String::new());
    Ok(StatusCode::NO_CONTENT)
}
