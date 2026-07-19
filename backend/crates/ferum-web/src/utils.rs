use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use axum::http::HeaderMap;

use crate::app_state::AppState;

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
