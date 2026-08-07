use axum::extract::{Extension, Multipart, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::utils::{read_image_field, validate_upload_image, ImageKind};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::constants::{MAX_FAVICON_BYTES, MAX_LOGO_BYTES};
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_application::storage_utils::{cas_key, validate_favicon_content_type};
use ferum_application::validators::validate_favicon_magic;

/// SMTP keys, editable from `/admin/settings` and applied without a restart via
/// `AppState::email`. Named once so the writable list, the reload path and the
/// startup wiring cannot drift.
pub const SMTP_HOST_KEY: &str = "smtp_host";
pub const SMTP_PORT_KEY: &str = "smtp_port";
pub const SMTP_USER_KEY: &str = "smtp_user";
pub const SMTP_PASS_KEY: &str = "smtp_pass";

/// Port used when `smtp_port` is absent from site_config — the SMTP submission
/// port, matching `Config`'s own default.
const DEFAULT_SMTP_PORT: u16 = 587;

/// Keys the config API will accept on write.
///
/// Deliberately a superset of [`CONFIG_READABLE_KEYS`]: `smtp_pass` is writable
/// but must never be echoed back. Anything absent here is silently dropped by
/// `update_config`, so a field rendered on the settings page and missing from
/// this list is a save that appears to succeed and does nothing.
const CONFIG_WRITABLE_KEYS: &[&str] = &[
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
    SMTP_HOST_KEY,
    SMTP_PORT_KEY,
    SMTP_USER_KEY,
    SMTP_PASS_KEY,
];

/// Keys `get_config` will return. `smtp_pass` is **not** among them — the settings
/// page only needs to know whether a password is set, which it reads from the
/// server-side `site_config_cache`, never from this endpoint.
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
    SMTP_HOST_KEY,
    SMTP_PORT_KEY,
    SMTP_USER_KEY,
];

/// Pulls the SMTP block out of a site_config map for [`ReloadableEmailService`].
///
/// `Ok(None)` means "not configured" (no host). `Err` means the stored values are
/// unusable, and the caller must not swap the live transport.
pub fn smtp_settings_from_config(
    cfg: &HashMap<String, String>,
) -> Result<Option<SmtpSettings>, AppError> {
    let host = cfg
        .get(SMTP_HOST_KEY)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(host) = host else {
        return Ok(None);
    };

    let port = match cfg.get(SMTP_PORT_KEY).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(raw) => raw
            .parse::<u16>()
            .map_err(|_| AppError::invalid("invalid_smtp_port"))?,
        None => DEFAULT_SMTP_PORT,
    };

    let non_blank = |key: &str| {
        cfg.get(key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    Ok(Some(SmtpSettings {
        host: host.to_string(),
        port,
        username: non_blank(SMTP_USER_KEY),
        password: non_blank(SMTP_PASS_KEY),
    }))
}

pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub async fn get_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_manage_config(actor)?;
    let all = state.site_config.get_all().await?;
    Ok(Json(DataResponse::new(readable_only(all))))
}

/// Strips every key not on [`CONFIG_READABLE_KEYS`], so internal/infra keys and
/// `smtp_pass` never leave the process through this endpoint.
fn readable_only(all: HashMap<String, String>) -> HashMap<String, String> {
    all.into_iter()
        .filter(|(k, _)| CONFIG_READABLE_KEYS.contains(&k.as_str()))
        .collect()
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
        .filter(|(k, _)| CONFIG_WRITABLE_KEYS.contains(&k.as_str()))
        .collect();

    // SMTP is applied to the live transport, so validate before writing anything:
    // an unparseable port must fail the request rather than persist and silently
    // leave mail broken. Merged over the current cache because the settings page
    // sends a partial block (a blank password means "keep the current one").
    let smtp_touched = filtered.keys().any(|k| {
        matches!(
            k.as_str(),
            SMTP_HOST_KEY | SMTP_PORT_KEY | SMTP_USER_KEY | SMTP_PASS_KEY
        )
    });
    let smtp_settings = if smtp_touched {
        let mut merged = state.site_config_cache.read().await.clone();
        merged.extend(filtered.iter().map(|(k, v)| (k.clone(), v.clone())));
        Some(smtp_settings_from_config(&merged)?)
    } else {
        None
    };

    state.site_config.set_many(&filtered).await?;
    {
        let mut cache = state.site_config_cache.write().await;
        for (k, v) in &filtered {
            cache.insert(k.clone(), v.clone());
        }
    }

    // Swap the transport only after the new values are durable, so a restart and
    // the running process always agree on which SMTP server is in use.
    if let Some(settings) = smtp_settings {
        let (host, port, user, pass) = match &settings {
            Some(s) => (
                Some(s.host.as_str()),
                s.port,
                s.username.as_deref(),
                s.password.as_deref(),
            ),
            None => (None, DEFAULT_SMTP_PORT, None, None),
        };
        state.email.reload(host, port, user, pass).await?;
    }

    let config = state.site_config.get_all().await?;
    Ok(Json(DataResponse::new(readable_only(config))))
}

/// POST /api/admin/config/favicon — upload a new favicon image.
pub async fn upload_favicon(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_manage_config(actor)?;

    // Not `validate_upload_image`: a favicon additionally accepts ICO and
    // rejects SVG, and its cap is expressed in KB rather than MB.
    let (data, content_type) = read_image_field(&mut multipart, "file").await?;
    if !validate_favicon_content_type(&content_type) || !validate_favicon_magic(&data) {
        return Err(AppError::invalid("favicon_invalid_type").into());
    }
    if data.len() > MAX_FAVICON_BYTES {
        return Err(AppError::invalid_with(
            "favicon_too_large",
            [("limit_kb", (MAX_FAVICON_BYTES / 1024).into())],
        )
        .into());
    }

    let key = cas_key("favicons", &data, &content_type);

    let size = data.len() as i64;
    state.storage.put(&key, data, &content_type).await?;
    state
        .stored_files
        .upsert_and_ref(&key, &content_type, size, Some(actor.id))
        .await?;

    let old_key = state
        .site_config
        .get("favicon_url")
        .await?
        .and_then(|url| state.storage.key_from_url(&url));

    // Persisted into site_config, so it must name the file rather than its
    // current location — see `ports::file_url`.
    let favicon_url = ferum_application::ports::file_url(&key);
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
        .and_then(|url| state.storage.key_from_url(&url))
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

    let (data, content_type) = read_image_field(&mut multipart, "file").await?;
    validate_upload_image(&content_type, &data, MAX_LOGO_BYTES, ImageKind::LOGO)?;

    let key = cas_key("logos", &data, &content_type);

    let size = data.len() as i64;
    state.storage.put(&key, data, &content_type).await?;
    state
        .stored_files
        .upsert_and_ref(&key, &content_type, size, Some(actor.id))
        .await?;

    let old_key = state
        .site_config
        .get("logo_url")
        .await?
        .and_then(|url| state.storage.key_from_url(&url));

    // Persisted into site_config — see `ports::file_url`.
    let logo_url = ferum_application::ports::file_url(&key);
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
        .and_then(|url| state.storage.key_from_url(&url))
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

// ── Homepage hero ───────────────────────────────────────────────────────────
//
// The curated hero-tile endpoints that used to live here (POST/PUT
// …/config/hero-tiles, backed by the `home_hero_tiles` site_config key) are gone.
// The homepage masthead is now the `home-hero` plugin, which keeps its content —
// copy and image URLs alike — in its own plugin config.
//
// One consequence worth knowing: the plugin's image URLs are plain strings and do
// NOT take a CAS reference the way tiles did. Nothing here reference-counts them,
// so a key whose last other reference disappears is collectable while the plugin
// still points at it. See examples/plugins/home-hero/README.md.
