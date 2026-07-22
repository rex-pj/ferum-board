use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, DefaultBodyLimit};
use axum::http::{Request, StatusCode};
use axum::http::{header, HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::Span;

use crate::app_state::AppState;
use crate::handlers::admin::api::plugins::get_active_slots;
use crate::handlers::pages::{render_404_page, render_error_page};
use crate::middleware::auth::auth_middleware;
use crate::middleware::AuthUser;
use crate::middleware::csrf::csrf_origin_check;
use crate::middleware::locale::{negotiate_locale, translate_errors};
use crate::middleware::rate_limit::RateLimitConfig;
use crate::middleware::security_headers::security_headers;
use crate::middleware::setup_guard::setup_guard;

mod admin_routes;
mod mod_routes;
mod public_routes;

use admin_routes::{admin_api_routes, admin_page_routes, files_routes, setup_routes};
use mod_routes::{mod_api_routes, mod_page_routes};
use public_routes::{api_routes, auth_routes, public_page_routes};

// ─── Error page handlers ──────────────────────────────────────────────────────

async fn page_not_found(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    axum::Extension(req_locale): axum::Extension<crate::middleware::locale::RequestLocale>,
) -> Response {
    render_404_page(&state, &req_locale, auth_user.as_ref()).await
}

// Intercepts 4xx/5xx responses on page routes and replaces them with themed
// error pages. API routes (/api/, /static/, /themes/, /files/, /plugins/) are
// skipped so their JSON error format is preserved.
async fn error_page_layer(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Read before the request is consumed. `negotiate_locale` runs outside this
    // layer, so the extension is already present; the default is a safety net
    // for any path where it somehow isn't.
    let req_locale = req
        .extensions()
        .get::<crate::middleware::locale::RequestLocale>()
        .cloned()
        .unwrap_or_default();

    let path = req.uri().path().to_owned();
    let skip = path.starts_with("/api/")
        || path.starts_with("/static/")
        || path.starts_with("/themes/")
        || path.starts_with("/plugins/")
        || path.starts_with("/files/");

    let response = next.run(req).await;

    if skip {
        return response;
    }

    match response.status() {
        StatusCode::NOT_FOUND => render_404_page(&state, &req_locale, auth_user.as_ref()).await,
        s if s.is_client_error() || s.is_server_error() => {
            render_error_page(&state, &req_locale, auth_user.as_ref()).await
        }
        _ => response,
    }
}

/// Replace token values in paths that carry one-time credentials with `[token]`
/// so that email verification and password reset tokens never appear in server logs.
fn mask_sensitive_path(path: &str) -> std::borrow::Cow<'_, str> {
    const SENSITIVE_PREFIXES: &[&str] = &[
        "/api/auth/verify-email/",
        "/api/auth/password-resets/",
    ];
    for prefix in SENSITIVE_PREFIXES {
        if path.starts_with(prefix) {
            return std::borrow::Cow::Owned(format!("{prefix}[token]"));
        }
    }
    std::borrow::Cow::Borrowed(path)
}

pub fn build_router(
    state: AppState,
    cors_origins: &str,
    app_url: &str,
    max_upload_size_mb: u64,
) -> Router {
    let cors = build_cors(cors_origins);

    // Axum 0.8 applies DefaultBodyLimit (2 MB by default) to the Multipart extractor.
    // We disable it here so that the per-handler size checks (MAX_AVATAR_BYTES,
    // MAX_COVER_BYTES, MAX_FPKG_SIZE, …) are the real per-type enforcers.
    //
    // The global backstop against memory exhaustion is RequestBodyLimitLayer below.
    // It must never sit below the largest *legitimate* upload:
    //   plugin package (.fpkg) = 50 MB, cover = 8 MB, avatar = 5 MB.
    // Per-handler limits remain the real, per-type enforcement.
    const PLUGIN_PACKAGE_CEILING_MB: u64 = 50;
    let body_cap_bytes = (max_upload_size_mb.max(PLUGIN_PACKAGE_CEILING_MB) as usize)
        .saturating_mul(1024 * 1024)
        .saturating_add(1024 * 1024); // multipart envelope/field overhead
    let body_limit = tower_http::limit::RequestBodyLimitLayer::new(body_cap_bytes);

    // Build the CSRF allowed-origin list: the server's own origin + every configured CORS origin.
    let csrf_allowed: Arc<Vec<String>> = {
        let mut origins: Vec<String> = cors_origins
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "*")
            .collect();
        let app = app_url.trim_end_matches('/').to_string();
        if !app.is_empty() && !origins.contains(&app) {
            origins.push(app);
        }
        Arc::new(origins)
    };

    let write_rl = Arc::new(RateLimitConfig::public_write());

    #[cfg(debug_assertions)]
    let router = Router::new()
        .route("/api/dev/reload-templates", axum::routing::post(crate::handlers::api::dev::reload_templates));
    #[cfg(not(debug_assertions))]
    let router = Router::new();

    let routed = router
        .route("/health", get(crate::handlers::api::health::health))
        .merge(files_routes())
        // Static files (Bootstrap, HTMX, Alpine, theme CSS, widgets).
        // Cache 1 day; ServeDir sets ETag + Last-Modified automatically so
        // browsers send conditional GETs after TTL and get 304 when unchanged.
        .nest_service(
            "/static",
            tower::ServiceBuilder::new()
                .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=86400"),
                ))
                .service(ServeDir::new(&state.static_dir).append_index_html_on_directories(false)),
        )
        // Theme assets: shorter TTL because themes can be reloaded at runtime
        .nest_service("/themes", ServeDir::new(&state.themes_dir))
        // Plugin assets: only files under {slug}/assets/ are served.
        // The full plugins_dir is NOT exposed via ServeDir to prevent leaking
        // manifests, hook scripts, and plugin configs to unauthenticated users.
        .route("/plugins/{slug}/assets/{*path}", axum::routing::get(crate::handlers::api::uploads::serve_plugin_asset))
        .merge(public_page_routes())
        .nest("/admin", admin_page_routes())
        .nest("/mod", mod_page_routes())
        .nest("/api/setup", setup_routes())
        .nest("/api/auth", auth_routes(state.clone()))
        .nest("/api", api_routes(state.clone(), write_rl.clone()))
        .nest("/api/mod", mod_api_routes(state.clone(), write_rl))
        .nest("/api/admin", admin_api_routes())
        .route("/api/plugins/active-slots", get(get_active_slots))
        .fallback(page_not_found)
        // Disable axum's 2 MB DefaultBodyLimit so Multipart uploads are governed
        // exclusively by RequestBodyLimitLayer (below) and the per-handler checks.
        // In axum 0.8, DefaultBodyLimit applies to Multipart; without disabling it,
        // files > ~2 MB cause an aborted TCP connection (ERR_CONNECTION_ABORTED in
        // Chrome), which the JS catch block surfaces as "Network error."
        .layer(DefaultBodyLimit::disable())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            setup_guard,
        ))
        .layer(middleware::from_fn(csrf_origin_check))
        .layer(axum::Extension(csrf_allowed))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            error_page_layer,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(cors)
        .layer(middleware::from_fn(security_headers))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request<_>| {
                    let request_id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    let ip = req
                        .extensions()
                        .get::<ConnectInfo<SocketAddr>>()
                        .map(|ci| ci.0.ip().to_string())
                        .unwrap_or_default();
                    // Mask token values in paths to prevent credential leakage in logs.
                    // e.g. /api/auth/verify-email/<token>  →  /api/auth/verify-email/[token]
                    //      /api/auth/password-resets/<token> → /api/auth/password-resets/[token]
                    let raw_path = req.uri().path();
                    let logged_path = mask_sensitive_path(raw_path);
                    tracing::info_span!(
                        "request",
                        request_id = %request_id,
                        method     = %req.method(),
                        path       = %logged_path,
                        ip         = %ip,
                        status     = tracing::field::Empty,
                        latency_ms = tracing::field::Empty,
                        user_id    = tracing::field::Empty,
                        username   = tracing::field::Empty,
                    )
                })
                .on_response(|res: &axum::http::Response<_>, latency: Duration, span: &Span| {
                    span.record("status", res.status().as_u16());
                    span.record("latency_ms", latency.as_millis());
                    tracing::info!(parent: span, "response_sent");
                })
                .on_failure(|error: tower_http::classify::ServerErrorsFailureClass, latency: Duration, span: &Span| {
                    span.record("latency_ms", latency.as_millis());
                    tracing::error!(
                        parent: span,
                        error = %error,
                        latency_ms = latency.as_millis(),
                        "request_failed"
                    );
                }),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(body_limit)
        .with_state(state.clone());

    // The locale layers wrap the *routed* router rather than being added to it.
    //
    // `Router::layer` attaches middleware to each endpoint, which means route
    // matching has already happened by the time it runs. `negotiate_locale`
    // rewrites the URI to strip a `/vi` prefix, so running it after matching is
    // useless — `/vi/forum` matches nothing and 404s before the prefix is ever
    // removed. Nesting the whole router behind a fallback_service puts these two
    // ahead of matching, which is where they have to be.
    //
    // Order: `negotiate_locale` outermost (it sets the `Locale` extension), then
    // `translate_errors`, which reads that extension and rewrites error bodies on
    // the way out — including errors from layers that never reach a handler.
    Router::new()
        .fallback_service(routed)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            translate_errors,
        ))
        .layer(middleware::from_fn_with_state(state, negotiate_locale))
}

/// Build a CORS layer from a comma-separated origin list.
/// Wildcard origin ("*" or empty) disables credentials to comply with RFC 6454.
/// An explicit origin list enables credentials (required for httpOnly cookie auth).
fn build_cors(origins: &str) -> CorsLayer {
    let trimmed = origins.trim();
    if trimmed.is_empty() || trimmed == "*" {
        // Wildcard: no credentials — browsers reject the combination anyway.
        return CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any);
        // allow_credentials intentionally omitted (defaults to false)
    }

    let parsed: Vec<HeaderValue> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
        .allow_credentials(true)
}
