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
/// The router disables axum's `DefaultBodyLimit` wholesale so that per-handler
/// checks (`MAX_AVATAR_BYTES`, `MAX_FPKG_SIZE`, …) are the real enforcers for
/// uploads. That is correct for uploads and badly wrong for everything else: it
/// also lifted the limit on every JSON endpoint, leaving the only backstop at
/// the global `RequestBodyLimitLayer` — sized for a 50 MB plugin package. An
/// unauthenticated `POST /api/auth/sessions` carrying a 50 MB JSON body was
/// enough to exhaust memory on the documented 2 vCPU / 4 GB target.
///
/// Discriminating on `Content-Type` rather than on the route keeps this in one
/// place. There are 18 multipart handlers spread across the public, admin and
/// mod routers; enumerating them here would silently 413 any upload route added
/// later and missed. A request that claims `multipart/form-data` passes through
/// to the per-handler byte checks exactly as before.
///
/// Note this cannot be expressed with `DefaultBodyLimit`, whose limit is chosen
/// when the layer is built, not per request.
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
