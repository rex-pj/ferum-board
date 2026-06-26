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
pub async fn render_admin(
    state: &AppState,
    template: &str,
    ctx: &Context,
) -> Result<Html<String>, PageError> {
    let mut ctx = ctx.clone();
    ctx.insert("default_theme_slug", DEFAULT_THEME_SLUG);
    let html = state.tera.render(template, &ctx).await?;
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

/// Parse a `YYYY-MM-DD` date input into a UTC timestamp at the start of that day.
pub fn parse_date_from(s: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    let d = chrono::NaiveDate::parse_from_str(s?.trim(), "%Y-%m-%d").ok()?;
    Some(chrono::DateTime::from_naive_utc_and_offset(
        d.and_hms_opt(0, 0, 0)?,
        chrono::Utc,
    ))
}

/// Parse a `YYYY-MM-DD` date input into a UTC timestamp at the end of that day
/// (inclusive upper bound).
pub fn parse_date_to(s: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    let d = chrono::NaiveDate::parse_from_str(s?.trim(), "%Y-%m-%d").ok()?;
    Some(chrono::DateTime::from_naive_utc_and_offset(
        d.and_hms_opt(23, 59, 59)?,
        chrono::Utc,
    ))
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
