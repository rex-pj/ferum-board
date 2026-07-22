//! Admin → Languages (Step 7 Tier 1: the locale roster).
//!
//! Two `site_config` keys would have been enough to *store* this, but a
//! self-hosted product needs somewhere to see and change it. What an admin
//! actually wants to know here is not "which locales exist" but "how much of the
//! site is actually translated in each" — so coverage is the headline number.
//!
//! Tier 2 (language-pack upload) and Tier 3 (per-string overrides) attach to this
//! page later; see docs/i18n-plan.md.

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
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

/// `site_config` key holding the comma-separated list of enabled locale tags.
/// Absent means "everything installed is enabled", which is the right default
/// for a site that has just dropped in a language pack.
pub const ENABLED_LOCALES_KEY: &str = "enabled_locales";
/// `site_config` key holding the tag served to visitors with no preference.
pub const DEFAULT_LOCALE_KEY: &str = "default_locale";

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
            let coverage = if total == 0 {
                100
            } else {
                ((translated * 100) / total) as u32
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
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("locales", &rows);
    ctx.insert("locales_dir", &state.locales_dir);

    render_admin(&state, &req_locale, "admin/languages.html", &ctx).await
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
            .site_config
            .set(DEFAULT_LOCALE_KEY, locale.as_str())
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
async fn enabled_locales(state: &AppState) -> Option<Vec<Locale>> {
    let raw = state.site_config.get(ENABLED_LOCALES_KEY).await.ok()??;
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
    state.site_config.set(ENABLED_LOCALES_KEY, &joined).await
}

async fn configured_default(state: &AppState) -> Locale {
    state
        .site_config
        .get(DEFAULT_LOCALE_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|t| Locale::parse(&t))
        .unwrap_or_default()
}
