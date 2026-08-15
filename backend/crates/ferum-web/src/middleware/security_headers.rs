//! Security response headers. `img-src` is NOT constant — startup.rs appends
//! the storage backend's origin, else every uploaded image is CSP-blocked.
//! Only img-src widens: a bucket serving scripts would make an upload-validation
//! bug into code execution.

use axum::extract::{Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// Adds defensive security headers to every response.
///
/// `script-src` needs `'unsafe-eval'` for Alpine's runtime expression compiler —
/// without it every `x-data` page renders blank. Inline `<script>` and `on*=`
/// stay forbidden. `style-src` allows `'unsafe-inline'` for Bootstrap.
/// `img-src` gains the storage origin at startup; see
/// [`SecurityHeadersConfig::new`].
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
    /// Origins `StorageService::public_url` can mint. Empty for same-origin.
    ///
    /// NOT a constant: a fixed `img-src 'self' data: blob:` silently breaks
    /// every non-same-origin upload config — the write succeeds, the browser
    /// refuses to load it, and nothing appears server-side.
    ///
    /// **Only `img-src` widens.** A bucket serving scripts to this origin would
    /// turn an upload-validation bug into code execution.
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
