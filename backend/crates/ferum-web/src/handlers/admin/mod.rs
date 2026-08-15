pub mod api;
pub mod pages;

use axum::response::Html;
use serde::Serialize;
use tera::Context;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::pages::PageError;
use crate::middleware::AuthUser;
use crate::view_models::page_context::SiteCtx;
use ferum_application::constants::DEFAULT_THEME_SLUG;

// ─── Shared helpers ────────────────────────────────────────────────────────────

fn hex_to_rgb(hex: &str) -> Option<String> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(format!("{r}, {g}, {b}"))
}

pub async fn site_ctx(state: &AppState) -> SiteCtx {
    let _lat = crate::telemetry::Latency::start("site_ctx");
    let configs = state.site_config_cache.read().await;
    crate::telemetry::record_cache_result("site_ctx", "rwlock", true);
    let non_empty = |k: &str| configs.get(k).filter(|v| !v.is_empty()).cloned();
    let primary_color = non_empty("primary_color");
    let primary_color_rgb = primary_color.as_deref().and_then(hex_to_rgb);
    SiteCtx {
        name: configs
            .get("site_name")
            .filter(|v| !v.is_empty())
            .cloned()
            .unwrap_or_else(|| "Ferum Board".to_string()),
        slogan: configs.get("site_slogan").cloned().unwrap_or_default(),
        tagline: configs.get("site_tagline").cloned().unwrap_or_default(),
        logo_url: non_empty("logo_url"),
        favicon_url: non_empty("favicon_url"),
        primary_color,
        primary_color_rgb,
        url: state.app_url.clone(),
    }
}

pub fn require_admin(auth_user: &AuthUser) -> Result<(), PageError> {
    let has_any_admin_perm = auth_user.permissions.iter().any(|p| p.starts_with("admin."));
    if !has_any_admin_perm {
        return Err(PageError::Unauthorized);
    }
    Ok(())
}

#[tracing::instrument(skip(state, ctx), fields(template))]
/// Renders an admin or moderator page in the request's locale.
///
/// Not the default locale: that would make the panels the one place a user's
/// language is ignored. Costs nothing while `adm-*` is English-only — those keys
/// fall back — and the panels translate themselves the moment anyone adds them.
pub async fn render_admin(
    state: &AppState,
    req_locale: &crate::middleware::locale::RequestLocale,
    template: &str,
    mut ctx: Context,
) -> Result<Html<String>, PageError> {
    let locale = &req_locale.locale;
    ctx.insert("default_theme_slug", DEFAULT_THEME_SLUG);
    ctx.insert("locale", locale.as_str());
    ctx.insert("current_path", &req_locale.canonical_path);
    // Drives the header's language switcher; the template hides it entirely when
    // only one language is installed, so a single-language site sees no control.
    ctx.insert(
        "available_locales",
        &state
            .translator
            .available_locales()
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>(),
    );
    // Admin page scripts call `Ferum.t()` from the same shared `ferum-utils.js`
    // the public pages use, so the dictionary has to be present here too —
    // otherwise every client-rendered admin message would show a raw key.
    ctx.insert("js_strings", &crate::handlers::pages::js_strings_for(state, locale));

    let html = state.tera.render(locale, template, ctx).await?;
    Ok(Html(html))
}

// ─── Shared query / context types ─────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct PageQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub actor_id: Option<String>,
    pub target_type: Option<String>,
    pub category_id: Option<String>,
    pub author_id: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

/// Parse a UUID query param, treating empty/invalid strings as "no filter".
pub fn parse_opt_uuid(s: Option<&str>) -> Option<Uuid> {
    s.filter(|v| !v.is_empty()).and_then(|v| v.parse().ok())
}

/// Resolves the instant a calendar day starts in `tz`, for both date filters.
///
/// Uses the site's `reporting_timezone`, so a filtered list and the dashboard
/// chart agree on what a day is — reading the input as UTC shifts the window by
/// the site's offset and silently returns a plausible-looking wrong set.
///
/// Handles zones where local midnight does not exist (America/Santiago shifts
/// *at* 00:00): `LocalResult::None` would drop the filter entirely.
fn day_start_in(date: chrono::NaiveDate, tz: chrono_tz::Tz) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;

    let at = |h: u32| -> Option<chrono::DateTime<chrono::Utc>> {
        let naive = date.and_hms_opt(h, 0, 0)?;
        tz.from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    };
    at(0).or_else(|| at(1))
}

/// Parses the site's reporting timezone, falling back to UTC.
///
/// A value that fails to parse here cannot normally exist — the config endpoint
/// validates it against Postgres before storing — so this is the belt to that
/// braces, and UTC is the right thing to fall back to rather than dropping the
/// filter entirely.
pub fn reporting_tz(raw: &str) -> chrono_tz::Tz {
    raw.trim().parse().unwrap_or(chrono_tz::UTC)
}

/// The site's reporting timezone, read from the in-process config cache.
///
/// The cache rather than the database: this is on the render path of three
/// filtered admin pages, and `update_config` refreshes the cache in the same
/// request that writes the row, so it cannot go stale.
pub async fn reporting_tz_of(state: &AppState) -> chrono_tz::Tz {
    let configs = state.site_config_cache.read().await;
    reporting_tz(configs.get("reporting_timezone").map_or("", |s| s.as_str()))
}

/// Parse a `YYYY-MM-DD` date input into the instant that day begins in `tz`.
///
/// Pairs with `>=`.
pub fn parse_date_from(s: Option<&str>, tz: chrono_tz::Tz) -> Option<chrono::DateTime<chrono::Utc>> {
    let d = chrono::NaiveDate::parse_from_str(s?.trim(), "%Y-%m-%d").ok()?;
    day_start_in(d, tz)
}

/// Parse a `YYYY-MM-DD` date input into the instant the **next** day begins in
/// `tz` — an *exclusive* upper bound.
///
/// This used to return 23:59:59 of the same day and be compared inclusively,
/// which is wrong at both ends: a row at 23:59:59.500 fell into neither Aug 8
/// nor Aug 9, and a row landing exactly on a shared boundary was counted in two
/// adjacent periods. A half-open `[start, end)` has neither gap nor overlap by
/// construction — see `CLAUDE.md` → Time and Timezones, rule 6.
///
/// **Callers must compare with `<`, not `<=`.**
pub fn parse_date_to(s: Option<&str>, tz: chrono_tz::Tz) -> Option<chrono::DateTime<chrono::Utc>> {
    let d = chrono::NaiveDate::parse_from_str(s?.trim(), "%Y-%m-%d").ok()?;
    day_start_in(d.succ_opt()?, tz)
}

/// Minimal {id, name} option used to populate static `<select>` filters.
#[derive(Serialize)]
pub struct SelectOptionCtx {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct ModeratorCtx {
    pub assignment_id: String,
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub granted_at: String,
}
