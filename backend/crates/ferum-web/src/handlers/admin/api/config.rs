use axum::extract::{Extension, Multipart, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::collections::{HashMap, HashSet};

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::utils::{read_image_field, validate_upload_image, ImageKind};
use crate::view_models::page_context::HeroTileCtx;
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::constants::{
    MAX_FAVICON_BYTES, MAX_HERO_CAPTION_LEN, MAX_HERO_IMAGE_BYTES, MAX_HERO_LINK_LEN,
    MAX_HERO_TILES, MAX_LOGO_BYTES,
};
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_application::storage_utils::{cas_key, validate_favicon_content_type};
use ferum_application::validators::{is_safe_external_link, validate_favicon_magic};

/// site_config key holding the curated hero tiles as a JSON array. Named once so
/// the reader (home page), the writer (below) and the whitelist cannot drift.
pub const HERO_TILES_KEY: &str = "home_hero_tiles";

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
    HERO_TILES_KEY,
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
    HERO_TILES_KEY,
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

/// Readable above, but NOT writable through the generic `PUT /api/admin/config`.
///
/// `home_hero_tiles` holds a JSON array whose `image_url`s are reference-counted
/// CAS keys. Letting it through the generic key/value writer would allow a
/// hand-edited string to drop an image's only reference without ever calling
/// `decrement_ref` (leaking the blob forever) or to point a tile at a file the
/// operator never uploaded. Both are prevented by routing every write through
/// `add_hero_tile` / `save_hero_tiles`, which own the ref-count bookkeeping.
///
/// `logo_url` / `favicon_url` are deliberately absent: the settings page exposes
/// them as free-text fields on purpose (an operator may point them at an external
/// CDN), and that behaviour predates this list.
const CONFIG_MANAGED_KEYS: &[&str] = &[HERO_TILES_KEY];

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
        .filter(|(k, _)| {
            CONFIG_WRITABLE_KEYS.contains(&k.as_str())
                && !CONFIG_MANAGED_KEYS.contains(&k.as_str())
        })
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

    let favicon_url = state.storage.public_url(&key);
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

    let logo_url = state.storage.public_url(&key);
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

// ── Curated homepage hero tiles ─────────────────────────────────────────────
//
// Two endpoints, split by who owns the CAS reference:
//
//   POST  …/hero-tiles  uploads one image and appends a tile in the same call.
//   PUT   …/hero-tiles  saves captions, links, order and deletions.
//
// The upload deliberately persists the tile immediately instead of handing back a
// URL for the client to submit later. A two-step "upload then save" would take a
// CAS reference on upload and leak it whenever the operator closed the tab before
// saving — there would be a blob no code path could ever free. Appending straight
// away means every reference this endpoint takes is already recorded in the
// config, so `save_hero_tiles` can always find and release it.

/// Read the stored tile list. A missing, empty, or malformed value yields an empty
/// list rather than an error: a corrupt config row must not take the homepage or
/// the settings page down, and the next save overwrites it cleanly.
async fn load_hero_tiles(state: &AppState) -> Vec<HeroTileCtx> {
    state
        .site_config
        .get(HERO_TILES_KEY)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
        .and_then(|v| serde_json::from_str::<Vec<HeroTileCtx>>(&v).ok())
        .unwrap_or_default()
}

/// Persist the tile list to both the durable store and the in-process cache that
/// `site_ctx`/the homepage read from, so the change is visible on the next request
/// without a restart.
async fn store_hero_tiles(state: &AppState, tiles: &[HeroTileCtx]) -> Result<String, AppError> {
    let json = serde_json::to_string(tiles)
        .map_err(|e| AppError::internal(format!("hero tile serialization failed: {e}")))?;
    state.site_config.set(HERO_TILES_KEY, &json).await?;
    state
        .site_config_cache
        .write()
        .await
        .insert(HERO_TILES_KEY.to_string(), json.clone());
    Ok(json)
}

/// Drop one reference to a CAS image and delete the blob when nothing else holds it.
///
/// Tiles store whatever URL the storage backend minted; `stored_files` is keyed
/// by the CAS key inside it. A URL the backend does not recognise (an
/// operator-set external link) simply has no CAS reference to release, so it is
/// skipped — which is also why this asks the backend rather than pattern
/// matching: only it can tell one of our URLs from somebody else's.
async fn release_hero_image(state: &AppState, image_url: &str) {
    let Some(key) = state.storage.key_from_url(image_url) else {
        return;
    };
    let key = key.as_str();
    if let Ok(remaining) = state.stored_files.decrement_ref(key).await {
        if remaining == 0 {
            let _ = state.stored_files.delete_by_key(key).await;
        }
    }
}

/// POST /api/admin/config/hero-tiles — upload an image and append it as a tile.
///
/// Returns the full updated list so the client re-renders from server truth rather
/// than guessing what the append did.
pub async fn add_hero_tile(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_manage_config(actor)?;

    let mut tiles = load_hero_tiles(&state).await;
    // Checked before reading the body so an over-budget upload is refused without
    // buffering megabytes of it first.
    if tiles.len() >= MAX_HERO_TILES {
        return Err(AppError::invalid_with(
            "hero_tiles_full",
            [("limit", MAX_HERO_TILES.into())],
        )
        .into());
    }

    let (data, content_type) = read_image_field(&mut multipart, "file").await?;
    validate_upload_image(&content_type, &data, MAX_HERO_IMAGE_BYTES, ImageKind::HERO_IMAGE)?;

    let key = cas_key("hero", &data, &content_type);
    let size = data.len() as i64;
    state.storage.put(&key, data, &content_type).await?;
    state
        .stored_files
        .upsert_and_ref(&key, &content_type, size, Some(actor.id))
        .await?;

    tiles.push(HeroTileCtx {
        image_url: state.storage.public_url(&key),
        link: String::new(),
        caption: String::new(),
    });
    store_hero_tiles(&state, &tiles).await?;

    Ok(Json(serde_json::json!({ "data": tiles })))
}

/// PUT /api/admin/config/hero-tiles — save captions, links, order and deletions.
///
/// Takes the whole list because order is meaningful (tile 1 is the mosaic's lead
/// image) and because a whole-list diff is what makes reference counting correct:
/// any image present before and absent now is released exactly once here.
pub async fn save_hero_tiles(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<Vec<HeroTileCtx>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    PermissionChecker::can_manage_config(actor)?;

    if body.len() > MAX_HERO_TILES {
        return Err(AppError::invalid_with(
            "hero_tiles_full",
            [("limit", MAX_HERO_TILES.into())],
        )
        .into());
    }

    let existing = load_hero_tiles(&state).await;

    let mut seen: HashSet<String> = HashSet::new();
    let mut tiles: Vec<HeroTileCtx> = Vec::with_capacity(body.len());
    for tile in body {
        // An image_url must be one this endpoint already knows about. The upload
        // endpoint is the only thing that mints them, so anything else is either a
        // stale client or an attempt to point a tile at an arbitrary stored file —
        // and it is also what stops a `javascript:`/`data:` URL reaching `src`.
        if !existing.iter().any(|e| e.image_url == tile.image_url) {
            return Err(AppError::invalid("hero_image_unknown").into());
        }
        // Two tiles sharing one image would make the delete diff release a
        // reference that is still in use.
        if !seen.insert(tile.image_url.clone()) {
            return Err(AppError::invalid("hero_image_duplicate").into());
        }

        let link = tile.link.trim().to_string();
        if !is_safe_external_link(&link) {
            return Err(AppError::invalid("hero_link_invalid").into());
        }
        if link.chars().count() > MAX_HERO_LINK_LEN {
            return Err(AppError::invalid_with(
                "hero_link_too_long",
                [("limit", MAX_HERO_LINK_LEN.into())],
            )
            .into());
        }

        let caption = tile.caption.trim().to_string();
        if caption.chars().count() > MAX_HERO_CAPTION_LEN {
            return Err(AppError::invalid_with(
                "hero_caption_too_long",
                [("limit", MAX_HERO_CAPTION_LEN.into())],
            )
            .into());
        }

        tiles.push(HeroTileCtx {
            image_url: tile.image_url,
            link,
            caption,
        });
    }

    store_hero_tiles(&state, &tiles).await?;

    // Release only after the new list is durably stored: if the write above fails
    // the images are still referenced by the list that is still live.
    for old in &existing {
        if !seen.contains(&old.image_url) {
            release_hero_image(&state, &old.image_url).await;
        }
    }

    Ok(Json(serde_json::json!({ "data": tiles })))
}
