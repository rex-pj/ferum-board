use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Validates the `Origin` header on state-changing requests (POST/PUT/PATCH/DELETE).
///
/// If `Origin` is present and not in the allowed list the request is rejected with
/// 403. Requests without an `Origin` header are allowed through — they originate
/// from server-to-server calls, curl, or old browsers that never send it, all of
/// which cannot carry the HttpOnly auth cookie cross-site.
///
/// The allowed list is built at startup from `APP_URL` + `CORS_ORIGINS`.
pub async fn csrf_origin_check(
    axum::extract::Extension(allowed): axum::extract::Extension<Arc<Vec<String>>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method();
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }

    if let Some(origin) = req.headers().get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok()) {
        if !allowed.iter().any(|a| a == origin) {
            let body = serde_json::json!({
                "error": {
                    "code": "forbidden",
                    "message": "CSRF check failed: request origin not allowed"
                }
            });
            return (StatusCode::FORBIDDEN, axum::Json(body)).into_response();
        }
    }

    next.run(req).await
}
