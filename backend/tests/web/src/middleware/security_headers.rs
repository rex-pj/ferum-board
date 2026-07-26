use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use ferum_web::middleware::security_headers::security_headers;

/// `https_enabled = false` mirrors a plain-HTTP dev deployment, which is what
/// every pre-existing assertion below was written against.
fn make_app() -> Router {
    make_app_with_https(false)
}

fn make_app_with_https(https_enabled: bool) -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            https_enabled,
            security_headers,
        ))
}

#[tokio::test]
async fn x_content_type_options_nosniff() {
    let resp = make_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["x-content-type-options"], "nosniff");
}

#[tokio::test]
async fn x_frame_options_sameorigin() {
    let resp = make_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.headers()["x-frame-options"], "SAMEORIGIN");
}

#[tokio::test]
async fn referrer_policy_is_set() {
    let resp = make_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(resp.headers().contains_key("referrer-policy"));
    let val = resp.headers()["referrer-policy"].to_str().unwrap();
    assert!(val.contains("origin"));
}

#[tokio::test]
async fn permissions_policy_is_set() {
    let resp = make_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(resp.headers().contains_key("permissions-policy"));
    let val = resp.headers()["permissions-policy"].to_str().unwrap();
    assert!(val.contains("camera") && val.contains("microphone"));
}

#[tokio::test]
async fn content_security_policy_is_set() {
    let resp = make_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(resp.headers().contains_key("content-security-policy"));
    let csp = resp.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    // `blob:` is load-bearing, not decoration: the product photo and thumbnail
    // pickers preview a freshly chosen file via `URL.createObjectURL`, which
    // yields a blob: URL. Drop it and every such preview renders as a broken
    // image with only a console warning to say why.
    assert!(csp.contains("img-src 'self' data: blob:"));
}

// ─── HSTS ────────────────────────────────────────────────────────────────────
// The app emits HSTS itself rather than relying on a reverse proxy that is not
// part of this repo, but only once TLS is actually in play.

#[tokio::test]
async fn hsts_is_set_when_https_enabled() {
    let resp = make_app_with_https(true)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let hsts = resp.headers()["strict-transport-security"].to_str().unwrap();
    assert!(hsts.contains("max-age=31536000"));
    assert!(hsts.contains("includeSubDomains"));
}

#[tokio::test]
async fn hsts_is_absent_over_plain_http() {
    // Sending HSTS from a plain-HTTP dev server would pin localhost to HTTPS in
    // the developer's browser and break every other project on the same host.
    let resp = make_app_with_https(false)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(!resp.headers().contains_key("strict-transport-security"));
}
