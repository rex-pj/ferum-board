//! Fixed-window per-IP rate limiting. Fails CLOSED: a backend error is a 503,
//! not a free pass, so an outage cannot silently disable brute-force limits.


use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::app_state::AppState;
use ferum_application::ports::RateLimitResult;

pub struct RateLimitConfig {
    /// Stable counter name shared by this group, per client.
    ///
    /// **Never key on `req.uri().path()`**: routes carry path parameters, so
    /// `/files/{key}` would give a fresh allowance per file and a scraper never
    /// hits the limit. A fixed name makes it per client, per capability — so
    /// auth is 10/min across login, registration and reset combined.
    pub bucket: &'static str,
    /// Key in site_config table that stores the per-minute limit for this route group.
    pub config_key: &'static str,
    /// Fallback value when the key is absent or unparseable.
    pub default_limit: u32,
    pub window: Duration,
}

impl RateLimitConfig {
    pub fn auth() -> Self {
        Self {
            bucket: "auth",
            config_key: "auth_rate_limit_per_min",
            default_limit: ferum_application::constants::DEFAULT_AUTH_RATE_LIMIT_PER_MIN,
            window: Duration::from_secs(60),
        }
    }

    pub fn public_write() -> Self {
        Self {
            bucket: "public_write",
            config_key: "public_write_rate_limit_per_min",
            default_limit: ferum_application::constants::DEFAULT_PUBLIC_WRITE_RATE_LIMIT_PER_MIN,
            window: Duration::from_secs(60),
        }
    }

    /// Limit for `/files/` blob reads.
    ///
    /// Far looser than the write limits, and necessarily so: one page can
    /// legitimately fetch dozens of avatars and thumbnails, so a write-shaped
    /// ceiling would break ordinary browsing. It exists to bound the abusive
    /// case — a scraper looping over `/files/`, which needs no account — not to
    /// pace a normal reader.
    ///
    /// Not written by `PgSystemSeedService`: absent means "use the default",
    /// and an admin who wants a different ceiling sets the key explicitly.
    pub fn file_read() -> Self {
        Self {
            bucket: "file_read",
            config_key: "file_read_rate_limit_per_min",
            default_limit: ferum_application::constants::DEFAULT_FILE_READ_RATE_LIMIT_PER_MIN,
            window: Duration::from_secs(60),
        }
    }
}

/// Extracts the real client IP.
///
/// At `trusted_proxy_count = 0` the TCP peer address is used and
/// `X-Forwarded-For` is ignored — otherwise anyone can forge it to bypass rate
/// limiting. At N > 0 the Nth-from-last XFF entry is the client, with
/// `X-Real-IP` as a fallback.
pub fn extract_client_ip(headers: &axum::http::HeaderMap, trusted_proxy_count: u32) -> String {
    if trusted_proxy_count > 0 {
        if let Some(xff) = headers
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
        {
            let parts: Vec<&str> = xff.split(',').map(str::trim).collect();
            let n = trusted_proxy_count as usize;
            if parts.len() >= n {
                let idx = parts.len() - n;
                return parts[idx].to_string();
            }
            // Fewer entries than expected proxy count — use the first (leftmost) entry
            if let Some(first) = parts.first() {
                return first.to_string();
            }
        }
        if let Some(real_ip) = headers
            .get("X-Real-IP")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
        {
            return real_ip.to_string();
        }
    }
    "unknown".to_string()
}

/// Builds the counter key for a `(group, client)` pair.
///
/// Split out so the keying rule can be asserted directly — the previous rule was
/// wrong in a way no integration test noticed, because it only shows up when two
/// requests that *should* share a budget are compared.
pub fn rate_limit_key(bucket: &str, ip: &str) -> String {
    format!("rl:{bucket}:{ip}")
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    axum::extract::Extension(config): axum::extract::Extension<Arc<RateLimitConfig>>,
    axum::extract::ConnectInfo(peer_addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    // Use peer address as fallback; only consult forwarded headers when trusted_proxy_count > 0.
    let ip = {
        let forwarded = extract_client_ip(req.headers(), state.trusted_proxy_count);
        if forwarded == "unknown" {
            peer_addr.ip().to_string()
        } else {
            forwarded
        }
    };
    // Keyed on the group name, not the request path — see `RateLimitConfig::bucket`.
    let key = rate_limit_key(config.bucket, &ip);

    let limit = {
        let cache = state.site_config_cache.read().await;
        cache.get(config.config_key)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(config.default_limit)
    };

    match state
        .rate_limiter
        .check(&key, limit, config.window)
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => next.run(req).await,
        Ok(RateLimitResult::Denied { retry_after }) => {
            let body = json!({
                "error": {
                    "code": "rate_limit_exceeded",
                    "message": "Too many requests. Please try again later."
                }
            });
            (
                StatusCode::TOO_MANY_REQUESTS,
                [("Retry-After", retry_after.as_secs().to_string())],
                axum::Json(body),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("rate limiter backend error — failing closed: {:?}", e);
            let body = json!({
                "error": {
                    "code": "service_unavailable",
                    "message": "Service temporarily unavailable. Please try again later."
                }
            });
            (StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response()
        }
    }
}
