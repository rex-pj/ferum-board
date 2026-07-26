use axum::extract::{Request, State};
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
/// `https_enabled` is `AppState::cookies_secure` — taken as a bare `bool` rather
/// than the whole `AppState` because that is genuinely all this needs, and it
/// keeps the middleware constructible in a test without standing up a database.
pub async fn security_headers(
    State(https_enabled): State<bool>,
    req: Request,
    next: Next,
) -> Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();

    // HSTS, but only once the deployment is actually HTTPS — `cookies_secure` is
    // derived from APP_URL's scheme, the same signal that gates the Secure cookie
    // flag. Sending this over plain HTTP in local dev would pin localhost to
    // HTTPS in the developer's browser and break it for every other project on
    // the same host.
    //
    // Previously delegated entirely to Nginx, which is not part of this repo: any
    // deployment terminating TLS elsewhere silently shipped without HSTS.
    // Emitting it here means the guarantee travels with the app. A reverse proxy
    // that also sets it is harmless — this only inserts when absent.
    if https_enabled && !headers.contains_key("strict-transport-security") {
        headers.insert(
            axum::http::header::HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

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
    // `img-src` includes `data:` for base64 avatar placeholders and `blob:` for
    // local previews of files the user has just picked but not yet uploaded
    // (`URL.createObjectURL`), used by the product photo and thumbnail pickers.
    // Both schemes only ever reference bytes the page already holds — neither
    // can pull in a remote origin.
    // `connect-src` covers SSE (/api/notifications/stream) and fetch() calls.
    headers.insert(
        axum::http::header::HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self' 'unsafe-eval'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; \
             connect-src 'self'; \
             font-src 'self' data:; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );

    res
}
