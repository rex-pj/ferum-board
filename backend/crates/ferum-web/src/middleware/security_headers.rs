use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// Adds defensive security headers to every response.
///
/// CSP: scripts restricted to `'self'` plus `'unsafe-eval'` (no `'unsafe-inline'`).
/// `'unsafe-eval'` is required because Alpine.js (standard build, used for all admin/theme
/// interactivity) compiles its `x-data` / `x-show` / `x-for` expressions at runtime via the
/// Function constructor. Without it the browser blocks every Alpine directive and fully
/// Alpine-driven pages (e.g. /admin/permissions) render blank. Inline `<script>` tags and
/// `on*=` attributes remain forbidden — only Alpine's evaluator is permitted.
/// Styles allow `'unsafe-inline'` because Bootstrap injects inline styles at runtime.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();

    headers.insert(
        axum::http::header::HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("SAMEORIGIN"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    // CSP: restrict scripts to same origin; allow inline styles for Bootstrap/theme tokens.
    // `img-src` includes `data:` for base64 avatar placeholders.
    // `connect-src` covers SSE (/api/notifications/stream) and fetch() calls.
    headers.insert(
        axum::http::header::HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self' 'unsafe-eval'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; \
             connect-src 'self'; \
             font-src 'self' data:; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );

    res
}
