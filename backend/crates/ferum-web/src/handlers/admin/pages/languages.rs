//! Admin → Languages (Step 7 Tier 1: the locale roster).
//!
//! Two `site_config` keys would have been enough to *store* this, but a
//! self-hosted product needs somewhere to see and change it. What an admin
//! actually wants to know here is not "which locales exist" but "how much of the
//! site is actually translated in each" — so coverage is the headline number.
//!
//! Language-pack upload and a per-string override editor would attach to this page
//! later; neither is built. See docs/i18n.md.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use serde::{Deserialize, Serialize};
use tera::Context;

use ferum_domain::models::role::perm;
use ferum_domain::Locale;

use super::super::{render_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{require_page_auth, PageError};
use crate::middleware::locale::{site_default_locale, DEFAULT_LOCALE_KEY};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

/// `site_config` key holding the comma-separated list of enabled locale tags.
/// Absent means "everything installed is enabled", which is the right default
/// for a site that has just dropped in a language pack.
pub const ENABLED_LOCALES_KEY: &str = "enabled_locales";

#[derive(Serialize)]
struct LocaleRow {
    tag: String,
    /// The language's name in its own language, so the roster is readable even
    /// to an admin who does not speak it.
    name: String,
    enabled: bool,
    is_default: bool,
    /// Share of the default locale's keys that this locale translates itself,
    /// 0–100. The default locale is 100 by definition.
    coverage: u32,
    translated: usize,
    total: usize,
}

pub async fn languages(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    if !auth_user.has_perm(perm::ADMIN_LANGUAGES) {
        return Err(PageError::Unauthorized);
    }

    let installed = state.translator.available_locales();
    let canonical_keys = state.translator.default_locale_keys();
    let enabled = enabled_locales(&state).await;
    let site_default = configured_default(&state).await;

    let rows: Vec<LocaleRow> = installed
        .iter()
        .map(|locale| {
            // Coverage counts only keys the locale translates *itself*. A key
            // inherited from the default locale renders in the wrong language,
            // so counting it would report a fully-translated site that isn't.
            let translated = canonical_keys
                .iter()
                .filter(|key| state.translator.has_key(locale, key))
                .count();
            let total = canonical_keys.len();
            let coverage = match (translated * 100).checked_div(total) {
                // No keys at all means nothing is untranslated, so report 100%
                // rather than 0% — an empty catalog is complete, not empty.
                None => 100,
                Some(pct) => pct as u32,
            };

            LocaleRow {
                tag: locale.to_string(),
                name: state
                    .translator
                    .translate(locale, &format!("language-name-{locale}"), &[]),
                enabled: enabled.as_ref().is_none_or(|e| e.contains(locale)),
                is_default: *locale == site_default,
                coverage,
                translated,
                total,
            }
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("locales", &rows);
    ctx.insert("locales_dir", &state.locales_dir);

    render_admin(&state, &req_locale, "admin/languages.html", ctx).await
}

#[derive(Deserialize)]
pub struct UpdateLocaleRequest {
    pub enabled: Option<bool>,
    pub make_default: Option<bool>,
}

/// `PATCH /api/admin/languages/{tag}` — enable/disable, or set as site default.
pub async fn update_locale(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    axum::extract::Path(tag): axum::extract::Path<String>,
    axum::Json(body): axum::Json<UpdateLocaleRequest>,
) -> crate::view_models::HandlerResult<impl IntoResponse> {
    let auth_user = auth_user.ok_or(ferum_domain::AppError::Unauthorized)?;
    if !auth_user.has_perm(perm::ADMIN_LANGUAGES) {
        return Err(ferum_domain::AppError::forbidden("permission_denied").into());
    }

    let locale =
        Locale::parse(&tag).ok_or_else(|| ferum_domain::AppError::invalid("locale_not_enabled"))?;
    if !state.translator.available_locales().contains(&locale) {
        return Err(ferum_domain::AppError::invalid("locale_not_enabled").into());
    }

    if body.make_default == Some(true) {
        // Making a locale the default implies enabling it — a default nobody can
        // be served is a broken state, so don't allow the UI to produce it.
        state
            .set_site_config(DEFAULT_LOCALE_KEY, locale.as_str())
            .await?;
        let mut enabled = enabled_locales(&state)
            .await
            .unwrap_or_else(|| state.translator.available_locales());
        if !enabled.contains(&locale) {
            enabled.push(locale.clone());
            write_enabled(&state, &enabled).await?;
        }
    }

    if let Some(enable) = body.enabled {
        let mut enabled = enabled_locales(&state)
            .await
            .unwrap_or_else(|| state.translator.available_locales());

        if enable {
            if !enabled.contains(&locale) {
                enabled.push(locale.clone());
            }
        } else {
            // Refuse to disable the last one, or the site default: either leaves
            // visitors with no language to be served at all.
            if enabled.len() <= 1 {
                return Err(ferum_domain::AppError::invalid("cannot_disable_last_locale").into());
            }
            if locale == configured_default(&state).await {
                return Err(
                    ferum_domain::AppError::invalid("cannot_disable_default_locale").into(),
                );
            }
            enabled.retain(|l| l != &locale);
        }
        write_enabled(&state, &enabled).await?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Enabled locales, or `None` when the admin has never restricted the set —
/// which means "all installed".
///
/// Read from the in-memory config cache rather than the table, so it cannot
/// disagree with `negotiate_locale`, which reads the same map. `write_enabled`
/// keeps the two in step.
async fn enabled_locales(state: &AppState) -> Option<Vec<Locale>> {
    let raw = state
        .site_config_cache
        .read()
        .await
        .get(ENABLED_LOCALES_KEY)
        .cloned()?;
    let parsed: Vec<Locale> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(Locale::parse)
        .collect();
    // An empty or entirely unparseable value is treated as unset rather than as
    // "no languages enabled", which would black-hole the whole site.
    (!parsed.is_empty()).then_some(parsed)
}

async fn write_enabled(state: &AppState, locales: &[Locale]) -> Result<(), ferum_domain::AppError> {
    let joined = locales
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(",");
    state.set_site_config(ENABLED_LOCALES_KEY, &joined).await
}

/// The site default, resolved exactly as `negotiate_locale` resolves it.
///
/// Shares [`site_default_locale`] rather than re-reading the key, because this page
/// is where an admin *sees* which locale is default: a second implementation could
/// show a checkmark on a locale visitors are never served. The installed check lives
/// in that function, which is why an uninstalled tag reads back as the source locale
/// here too.
async fn configured_default(state: &AppState) -> Locale {
    let installed = state.translator.available_locales();
    let raw = state
        .site_config_cache
        .read()
        .await
        .get(DEFAULT_LOCALE_KEY)
        .cloned();
    site_default_locale(raw.as_deref(), &installed)
}
