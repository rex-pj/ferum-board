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
/// `img-src` includes `data:` for base64 avatar placeholders and `blob:` for local
/// previews of files the user has just picked (`URL.createObjectURL`), plus any
/// object-store / CDN origin uploads are actually served from — see
/// [`SecurityHeadersConfig::new`]. `connect-src` covers SSE
/// (/api/notifications/stream) and `fetch()` calls.
///
/// State is [`SecurityHeadersConfig`] rather than the whole `AppState` because
/// that is genuinely all this needs, and it keeps the middleware constructible
/// in a test without standing up a database.
pub async fn security_headers(
    State(config): State<SecurityHeadersConfig>,
    req: Request,
    next: Next,
) -> Response {
    let https_enabled = config.https_enabled;
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
    headers.insert(
        axum::http::header::HeaderName::from_static("content-security-policy"),
        config.csp.clone(),
    );

    res
}

/// Everything the middleware needs, resolved once at startup.
///
/// Was a bare `bool`. It grew a second field because the CSP is no longer a
/// constant: `img-src` has to name the origin that images are actually served
/// from, and that is a deployment decision.
#[derive(Clone)]
pub struct SecurityHeadersConfig {
    /// `AppState::cookies_secure` — derived from APP_URL's scheme, and the same
    /// signal that gates the Secure cookie flag.
    pub https_enabled: bool,
    /// Precomputed so no response pays for formatting a header it always sends.
    pub csp: HeaderValue,
}

/// CSP with no external image origin — the same-origin deployment, and the
/// fallback if a configured origin cannot go into a header.
const BASE_CSP: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-eval'; \
     style-src 'self' 'unsafe-inline'; \
     img-src 'self' data: blob:; \
     connect-src 'self'; \
     font-src 'self' data:; \
     frame-ancestors 'none'; \
     base-uri 'self'; \
     form-action 'self'";

impl SecurityHeadersConfig {
    /// `image_origins` are scheme+host origins (`https://cdn.example.com`) that
    /// `StorageService::public_url` can mint. Empty for a same-origin install.
    ///
    /// **Why this is not a constant any more.** `img-src 'self' data: blob:`
    /// silently breaks every deployment whose uploads are not same-origin — S3,
    /// GCS, and plain database storage behind `CDN_BASE_URL` alike. The upload
    /// succeeds, the row is written, `public_url` returns the right address, and
    /// the browser then refuses to load it. There is no server-side error and
    /// nothing in the logs; the only evidence is a console violation on a page
    /// full of blank images.
    ///
    /// Only `img-src` is widened. `script-src` and `connect-src` stay `'self'`:
    /// an object store holds user-uploaded bytes, and letting a bucket serve
    /// scripts to this origin would make an upload bug into code execution.
    pub fn new(https_enabled: bool, image_origins: &[String]) -> Self {
        let csp = if image_origins.is_empty() {
            HeaderValue::from_static(BASE_CSP)
        } else {
            let widened = BASE_CSP.replace(
                "img-src 'self' data: blob:",
                &format!("img-src 'self' data: blob: {}", image_origins.join(" ")),
            );
            HeaderValue::from_str(&widened).unwrap_or_else(|_| {
                // An origin containing a control character or non-ASCII byte
                // cannot go into a header. Falling back to the strict policy
                // breaks images, which is visible; emitting no CSP at all would
                // not be, so it is the wrong way to fail.
                tracing::error!(
                    "storage origin(s) {:?} cannot be put in a CSP header — \
                     falling back to same-origin img-src, which will block them",
                    image_origins
                );
                HeaderValue::from_static(BASE_CSP)
            })
        };
        Self { https_enabled, csp }
    }
}

/// Scheme + authority of an absolute URL, which is the form a CSP host-source
/// takes. `None` for a relative URL — `DatabaseStorageService` without a CDN
/// returns `/files/…`, and that needs no entry because it is already `'self'`.
pub fn csp_origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split('/').next()?;
    (!scheme.is_empty() && !authority.is_empty())
        .then(|| format!("{scheme}://{authority}"))
}
