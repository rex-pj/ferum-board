#![allow(dead_code)]

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
    pub limit: u32,
    pub window: Duration,
}

impl RateLimitConfig {
    pub fn auth() -> Self {
        Self {
            limit: 10,
            window: Duration::from_secs(60),
        }
    }

    pub fn public_write() -> Self {
        Self {
            limit: 30,
            window: Duration::from_secs(60),
        }
    }
}

/// Extract the real client IP from headers.
/// When behind a trusted reverse proxy the proxy appends the actual client IP
/// to X-Forwarded-For, so we take the LAST entry which is the one the trusted
/// proxy added. An attacker can only forge entries before it.
pub fn extract_client_ip(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').last())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    axum::extract::Extension(config): axum::extract::Extension<Arc<RateLimitConfig>>,
    req: Request,
    next: Next,
) -> Response {
    let ip = extract_client_ip(req.headers());
    let key = format!("rl:{}:{}", req.uri().path(), ip);

    match state
        .rate_limiter
        .check(&key, config.limit, config.window)
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
        Err(_) => next.run(req).await,
    }
}
