use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use ferum_web::middleware::security_headers::security_headers;

fn make_app() -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn(security_headers))
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
}
