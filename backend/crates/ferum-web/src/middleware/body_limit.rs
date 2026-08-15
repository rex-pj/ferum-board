use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Ceiling for any request body that is *not* a file upload.
///
/// Generous for JSON — the largest legitimate payload is a long Markdown post,
/// itself capped well below this by `MAX_POST_CONTENT_BYTES` — and small enough
/// that a flood of concurrent requests cannot exhaust memory.
const MAX_NON_UPLOAD_BODY_BYTES: usize = 1024 * 1024; // 1 MiB

/// Applies a small body limit to everything except multipart uploads.
///
/// The router disables `DefaultBodyLimit` so per-handler checks enforce upload
/// sizes — which also lifted it from every JSON endpoint, leaving only the 50 MB
/// plugin-package backstop. An unauthenticated 50 MB login body was enough to
/// exhaust the documented 2 vCPU / 4 GB target.
///
/// Discriminates on `Content-Type`, not route: the multipart handlers are spread
/// across the public, admin and mod routers, so an allowlist would silently 413
/// the next upload route someone adds.
pub async fn non_upload_body_limit(req: Request, next: Next) -> Response {
    let is_multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_start().starts_with("multipart/form-data"))
        .unwrap_or(false);

    if is_multipart {
        return next.run(req).await;
    }

    // Reject a declared oversize body before reading a single byte of it.
    if let Some(len) = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if len > MAX_NON_UPLOAD_BODY_BYTES {
            return payload_too_large();
        }
    }

    // A missing or lying Content-Length (chunked encoding) is handled by wrapping
    // the stream itself, so the cap holds regardless of what the client declared.
    let (parts, body) = req.into_parts();
    let limited = http_body_util::Limited::new(body, MAX_NON_UPLOAD_BODY_BYTES);
    let req = Request::from_parts(parts, Body::new(limited));

    next.run(req).await
}

/// Mirrors the JSON error shape every other middleware returns, so a client
/// parsing `error.code` gets something consistent rather than an empty 413.
fn payload_too_large() -> Response {
    let body = serde_json::json!({
        "error": {
            "code": "payload_too_large",
            "message": "Request body exceeds the maximum allowed size."
        }
    });
    (StatusCode::PAYLOAD_TOO_LARGE, axum::Json(body)).into_response()
}
