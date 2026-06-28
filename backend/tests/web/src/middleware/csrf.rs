use std::sync::Arc;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use tower::ServiceExt;
use ferum_web::middleware::csrf::csrf_origin_check;

fn make_app(allowed: Vec<String>) -> Router {
    Router::new()
        .route("/", post(|| async { "ok" }))
        .layer(axum::middleware::from_fn(csrf_origin_check))
        .layer(axum::Extension(Arc::new(allowed)))
}

// ─── GET/HEAD/OPTIONS pass without checking Origin ────────────────────────────

#[tokio::test]
async fn get_request_without_origin_passes() {
    let app = make_app(vec!["https://forum.example.com".to_string()]);
    let resp = app
        .oneshot(Request::builder().method("GET").uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // GET is allowed through; router returns 405 on POST-only route, but 404 would be blocked by
    // CSRF, so a non-403 status proves CSRF let it through.
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── POST without Origin header passes (server-to-server / curl) ──────────────

#[tokio::test]
async fn post_without_origin_header_passes_csrf() {
    let app = make_app(vec!["https://forum.example.com".to_string()]);
    let resp = app
        .oneshot(Request::builder().method("POST").uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── POST with matching Origin passes ─────────────────────────────────────────

#[tokio::test]
async fn post_with_allowed_origin_passes() {
    let app = make_app(vec!["https://forum.example.com".to_string()]);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("origin", "https://forum.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── POST with non-matching Origin is rejected ────────────────────────────────

#[tokio::test]
async fn post_with_disallowed_origin_returns_403() {
    let app = make_app(vec!["https://forum.example.com".to_string()]);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("origin", "https://evil.attacker.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn patch_with_disallowed_origin_returns_403() {
    let app = Router::new()
        .route("/", axum::routing::patch(|| async { "ok" }))
        .layer(axum::middleware::from_fn(csrf_origin_check))
        .layer(axum::Extension(Arc::new(vec!["https://forum.example.com".to_string()])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/")
                .header("origin", "https://evil.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_with_disallowed_origin_returns_403() {
    let app = Router::new()
        .route("/", axum::routing::delete(|| async { "ok" }))
        .layer(axum::middleware::from_fn(csrf_origin_check))
        .layer(axum::Extension(Arc::new(vec!["https://forum.example.com".to_string()])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/")
                .header("origin", "https://evil.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Multiple allowed origins ─────────────────────────────────────────────────

#[tokio::test]
async fn second_allowed_origin_also_passes() {
    let app = make_app(vec![
        "https://forum.example.com".to_string(),
        "https://staging.example.com".to_string(),
    ]);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("origin", "https://staging.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}
