use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use axum::extract::Multipart;
use axum::http::HeaderMap;
use ferum_application::shared::AppError;

use crate::app_state::AppState;

/// Pull a single image part out of a multipart body, returning its bytes and
/// declared content type. Size and magic-byte validation belong to the use case,
/// which owns the per-feature limits.
pub async fn read_image_field(
    multipart: &mut Multipart,
    field_name: &str,
) -> Result<(bytes::Bytes, String), AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some(field_name) {
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
            return Ok((data, content_type));
        }
    }
    Err(AppError::invalid("image_field_missing"))
}

// ─── Auth cookies ─────────────────────────────────────────────────────────────
// Single source for the auth Set-Cookie strings. Max-Age comes from the
// TokenService TTLs (JWT_EXPIRY_SECONDS / REFRESH_TOKEN_EXPIRY_DAYS) so the
// cookie lifetime always matches the token's `exp` claim.

/// `token=…` access-token cookie (Path=/).
pub fn access_token_cookie(state: &AppState, token: &str) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    format!(
        "token={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{secure}",
        state.token_service.access_token_ttl_secs()
    )
}

/// `refresh_token=…` cookie, scoped to the auth endpoints only.
pub fn refresh_token_cookie(state: &AppState, token: &str) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    format!(
        "refresh_token={token}; HttpOnly; SameSite=Lax; Path=/api/auth; Max-Age={}{secure}",
        state.token_service.refresh_token_ttl_secs()
    )
}

/// Expired `token=` cookie (logout).
pub fn clear_access_token_cookie(state: &AppState) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    format!("token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure}")
}

/// Expired `refresh_token=` cookie (logout).
pub fn clear_refresh_token_cookie(state: &AppState) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    format!("refresh_token=; HttpOnly; SameSite=Lax; Path=/api/auth; Max-Age=0{secure}")
}

/// Builds a 16-hex-char fingerprint for a guest viewer from IP + User-Agent.
///
/// - IP: read from `X-Forwarded-For` (Nginx) or `X-Real-IP`.
/// - User-Agent: tells apart different browsers on the same IP (e.g. an office behind NAT).
/// - Uses std DefaultHasher — no crypto needed, just enough dispersion.
/// - Returns `None` when both IP and UA are empty (not enough signal to dedup).
pub fn guest_fingerprint(headers: &HeaderMap) -> Option<String> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
        })
        .unwrap_or("");

    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if ip.is_empty() && ua.is_empty() {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    ip.hash(&mut hasher);
    ua.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// `ferum_locale=…` cookie carrying the visitor's chosen language.
///
/// Deliberately **not** `HttpOnly`: unlike the auth token this is not a secret,
/// and the client-side switcher needs to read it to show which language is
/// active before any JS state exists.
///
/// For a signed-in user this cookie is a *cache* of `user_preferences.locale`,
/// refreshed at login and whenever the preference changes. Keeping the durable
/// copy in the database is what makes the choice follow the account to a new
/// device; keeping the request-time copy in a cookie is what avoids a database
/// lookup on every single page render just to know which language to draw.
pub fn locale_cookie(state: &AppState, locale: &str) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    // One year: a language choice is not session state, and re-picking it on
    // every visit would be worse than the marginal privacy cost.
    format!("{}={locale}; SameSite=Lax; Path=/; Max-Age=31536000{secure}",
        crate::middleware::locale::LOCALE_COOKIE)
}

/// Expired `ferum_locale=` cookie — returns the visitor to negotiated default.
pub fn clear_locale_cookie(state: &AppState) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    format!("{}=; SameSite=Lax; Path=/; Max-Age=0{secure}",
        crate::middleware::locale::LOCALE_COOKIE)
}
