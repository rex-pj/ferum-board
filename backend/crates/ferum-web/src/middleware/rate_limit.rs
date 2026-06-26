
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
    /// Key in site_config table that stores the per-minute limit for this route group.
    pub config_key: &'static str,
    /// Fallback value when the key is absent or unparseable.
    pub default_limit: u32,
    pub window: Duration,
}

impl RateLimitConfig {
    pub fn auth() -> Self {
        Self {
            config_key: "auth_rate_limit_per_min",
            default_limit: ferum_application::constants::DEFAULT_AUTH_RATE_LIMIT_PER_MIN,
            window: Duration::from_secs(60),
        }
    }

    pub fn public_write() -> Self {
        Self {
            config_key: "public_write_rate_limit_per_min",
            default_limit: ferum_application::constants::DEFAULT_PUBLIC_WRITE_RATE_LIMIT_PER_MIN,
            window: Duration::from_secs(60),
        }
    }
}

/// Extract the real client IP.
///
/// When `trusted_proxy_count` is 0 (default), the raw TCP peer address is always
/// used — `X-Forwarded-For` is ignored. This is the safe default: without a
/// trusted proxy, an attacker can forge `X-Forwarded-For` to spoof their IP and
/// bypass rate limiting.
///
/// When `trusted_proxy_count` is N > 0, the (N)th-from-last entry in
/// `X-Forwarded-For` is used as the client IP. A correctly configured Nginx
/// appends the real peer address, so count=1 gives the actual client.
///
/// `X-Real-IP` is read only when `trusted_proxy_count` > 0, as a fallback
/// when `X-Forwarded-For` is absent (some proxy configs set only this header).
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
    let key = format!("rl:{}:{}", req.uri().path(), ip);

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
