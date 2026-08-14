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
///
/// Defined in `ferum-application` rather than here because the repository that
/// encrypts `smtp_pass` at rest also needs the key name, and infrastructure
/// cannot import this crate. Re-exported so every call site keeps its spelling.
pub use ferum_application::constants::{
    FROM_EMAIL_KEY, MAIL_PROVIDER_KEY, SMTP_HOST_KEY, SMTP_PASS_KEY, SMTP_PORT_KEY, SMTP_USER_KEY,
};
use ferum_application::ports::EmailService;
use ferum_infrastructure::email::{
    validate_from_address, MailReload, ResendEmailService, SelectedProvider, SmtpEndpoint,
};
use std::sync::Arc;

/// Port used when `smtp_port` is absent from site_config — the SMTP submission
/// port, matching `Config`'s own default.
const DEFAULT_SMTP_PORT: u16 = 587;

/// The IANA zone that defines the day boundary for every analytics query.
pub const REPORTING_TIMEZONE_KEY: &str = "reporting_timezone";

/// Rejects a `reporting_timezone` that either consumer cannot use.
///
/// **There are two consumers, and they do not accept the same set of names.**
/// That is the whole reason this function is more than one line:
///
/// * **PostgreSQL** interpolates it into `AT TIME ZONE` on every stats query. An
///   unknown zone there is a hard SQL error, so a typo takes out the entire
///   dashboard with nothing on the settings page explaining why.
/// * **`chrono-tz`** parses it in `handlers::admin::reporting_tz` to turn the
///   admin date-range filters into instants. An unknown zone there does **not**
///   error — it falls back to UTC.
///
/// The second failure mode is the dangerous one, and checking only Postgres
/// permits it. Postgres accepts `+07` and `posix/America/New_York`; `chrono-tz`
/// parses neither. Save `+07` and the dashboard chart buckets at UTC+7 while the
/// thread-list date filter buckets at UTC — two parts of the same admin panel
/// silently disagreeing about what a day is, which is precisely the class of bug
/// the reporting timezone exists to remove.
///
/// So both are required, and the Postgres probe *is* the operation rather than a
/// lookup in `pg_timezone_names` — it accepts exactly what the stats queries
/// will accept.
async fn validate_reporting_timezone(
    db: &sea_orm::DatabaseConnection,
    tz: &str,
) -> Result<(), AppError> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    let reject = || {
        AppError::invalid_with(
            "invalid_timezone",
            [("tz", ferum_domain::i18n::TransArg::Str(tz.to_string()))],
        )
    };

    // Cheap and local, so first.
    if tz.parse::<chrono_tz::Tz>().is_err() {
        return Err(reject());
    }

    let probe = Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT now() AT TIME ZONE $1",
        [tz.into()],
    );
    db.query_one_raw(probe).await.map_err(|_| reject())?;
    Ok(())
}

/// Keys the config API will accept on write.
///
/// Deliberately a superset of [`CONFIG_READABLE_KEYS`]: `smtp_pass` is writable
/// but must never be echoed back. Anything absent here is silently dropped by
/// `update_config`, so a field rendered on the settings page and missing from
/// this list is a save that appears to succeed and does nothing.
/// `pub` so `settings_template_contract.rs` can assert that every `cfg-*` field
/// rendered on the settings page appears here. The warning above is otherwise
/// unenforceable, and its failure mode — a save that reports success and changes
/// nothing — leaves no trace to debug from.
pub const CONFIG_WRITABLE_KEYS: &[&str] = &[
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
    "reporting_timezone",
    "max_posts_per_page",
    "max_threads_per_page",
    MAIL_PROVIDER_KEY,
    FROM_EMAIL_KEY,
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
    "reporting_timezone",
    "max_posts_per_page",
    "max_threads_per_page",
    MAIL_PROVIDER_KEY,
    FROM_EMAIL_KEY,
    SMTP_HOST_KEY,
    SMTP_PORT_KEY,
    SMTP_USER_KEY,
];

/// Keys whose **value** must never reach a template context or an API response.
///
/// [`CONFIG_READABLE_KEYS`] already keeps them out of `GET /api/admin/config`.
/// This list closes the other door: the settings *page* renders from
/// `site_config_cache` directly, which is the whole table — so the allowlist that
/// guards the JSON endpoint does not apply there at all.
pub const CONFIG_SECRET_KEYS: &[&str] = &[SMTP_PASS_KEY];

/// Splits a site_config map into the part safe to render and a per-secret
/// "is it set" flag.
///
/// Same shape as `WebhookResponse::has_secret` (`view_models/webhook.rs`): the UI
/// needs to know *whether* a secret exists so it can say "(set — enter a new
/// value to change)", and that is all it needs. Handing it the value and trusting
/// every present and future template not to print it is not a guarantee, it is a
/// convention — and this codebase already has one page whose author got it right
/// by luck rather than by construction.
///
/// "Set" means **non-blank**, not merely present: `PgSystemSeedService` seeds the
/// SMTP keys as `""`, so a presence test alone would report a password that was
/// never entered.
pub fn split_secrets(
    all: HashMap<String, String>,
) -> (HashMap<String, String>, HashMap<&'static str, bool>) {
    let mut safe = all;
    let flags = CONFIG_SECRET_KEYS
        .iter()
        .map(|key| {
            let present = safe
                .remove(*key)
                .is_some_and(|v| !v.trim().is_empty());
            (*key, present)
        })
        .collect();
    (safe, flags)
}

/// Pulls the SMTP block out of a site_config map for [`ReloadableEmailService`].
///
/// `Ok(None)` means "not configured" (no host). `Err` means the stored values are
/// unusable, and the caller must not swap the live transport.
pub fn smtp_settings_from_config(
    cfg: &HashMap<String, String>,
) -> Result<Option<SmtpEndpoint>, AppError> {
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

    Ok(Some(SmtpEndpoint {
        host: host.to_string(),
        port,
        username: non_blank(SMTP_USER_KEY),
        password: non_blank(SMTP_PASS_KEY),
    }))
}

/// Builds a whole [`MailReload`] from stored settings plus the env-only Resend key.
///
/// The Resend provider is constructed **here** rather than inside
/// `ReloadableEmailService` because its two inputs come from different places: the
/// API key is env-only and immutable for the process lifetime, while `from` is a
/// site_config value an admin can edit. Only this layer holds both — and the
/// consequence worth stating is that editing `from_email` rebuilds the Resend
/// client too, because `from` is captured at construction.
///
/// `resend_api_key` being `None` does not fail a Resend selection: it resolves to
/// `MailProvider::Disabled`, which is a state the settings page can render and the
/// operator can fix. Failing here would reject the save that was fixing it.
pub fn mail_reload_from_config(
    cfg: &HashMap<String, String>,
    resend_api_key: Option<&str>,
) -> Result<MailReload, AppError> {
    let from = cfg
        .get(FROM_EMAIL_KEY)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::invalid("from_email_required"))?;
    validate_from_address(from).map_err(|_| AppError::invalid("from_email_invalid"))?;

    let resend = match resend_api_key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => Some(Arc::new(ResendEmailService::new(key, from)?) as Arc<dyn EmailService>),
        None => None,
    };

    Ok(MailReload {
        selected: SelectedProvider::from_label(
            cfg.get(MAIL_PROVIDER_KEY).map(String::as_str).unwrap_or(""),
        ),
        from: from.to_string(),
        smtp: smtp_settings_from_config(cfg)?,
        resend,
    })
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

    let mut filtered: HashMap<String, String> = body
        .into_iter()
        .filter(|(k, _)| CONFIG_WRITABLE_KEYS.contains(&k.as_str()))
        .collect();

    // Store the timezone exactly as validated. Validating a trimmed value and
    // then persisting the untrimmed one means the two can differ, and the
    // difference would only surface at the next read.
    if let Some(tz) = filtered.get_mut(REPORTING_TIMEZONE_KEY) {
        *tz = tz.trim().to_string();
    }

    // Same rule as SMTP below: validate before writing anything. A bad zone that
    // reached the table would break every dashboard query until someone guessed
    // why, and the settings page would show it saved successfully.
    if let Some(tz) = filtered.get(REPORTING_TIMEZONE_KEY) {
        validate_reporting_timezone(&state.db, tz).await?;
    }

    // SMTP is applied to the live transport, so validate before writing anything:
    // an unparseable port must fail the request rather than persist and silently
    // leave mail broken. Merged over the current cache because the settings page
    // sends a partial block (a blank password means "keep the current one").
    // `from_email` and `mail_provider` belong in this set, not only the SMTP
    // fields: `from` is captured by each provider at construction, so editing it
    // without a reload leaves the live transport sending as the old address while
    // the page shows the new one.
    let mail_touched = filtered.keys().any(|k| {
        matches!(
            k.as_str(),
            MAIL_PROVIDER_KEY
                | FROM_EMAIL_KEY
                | SMTP_HOST_KEY
                | SMTP_PORT_KEY
                | SMTP_USER_KEY
                | SMTP_PASS_KEY
        )
    });
    // Validated against the MERGED map before anything is persisted, so a bad
    // value is rejected rather than stored and then failed on reload.
    let mail_reload = if mail_touched {
        let mut merged = state.site_config_cache.read().await.clone();
        merged.extend(filtered.iter().map(|(k, v)| (k.clone(), v.clone())));
        Some(mail_reload_from_config(
            &merged,
            state.resend_api_key.as_deref(),
        )?)
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

    // Swap the provider only after the new values are durable, so a restart and
    // the running process always agree on how mail is being sent.
    if let Some(reload) = mail_reload {
        state.email.reload(reload).await?;
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
