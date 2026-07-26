//! Covers the non-upload body cap added to close the unbounded-JSON DoS.
//!
//! The router disables axum's `DefaultBodyLimit` outright so that per-handler
//! byte checks govern uploads. That is right for uploads and was badly wrong for
//! everything else: it also uncapped every JSON endpoint down to the 50 MB
//! global backstop, so one unauthenticated request could pin ~50 MB of memory.
//!
//! Both directions matter here. Capping JSON is the fix; leaving multipart alone
//! is the thing that must not regress, because that is what would silently 413
//! every avatar and plugin-package upload in the product.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use tower::ServiceExt;

use ferum_web::middleware::body_limit::non_upload_body_limit;

/// Echoes the body back, so a request that is *not* rejected proves the body
/// actually streamed through rather than merely reaching the handler.
fn make_app() -> Router {
    Router::new()
        .route(
            "/",
            post(|body: axum::body::Bytes| async move { format!("{}", body.len()) }),
        )
        .layer(axum::middleware::from_fn(non_upload_body_limit))
}

const OVER_LIMIT: usize = 2 * 1024 * 1024; // 2 MiB — over the 1 MiB cap
const UNDER_LIMIT: usize = 1024; // comfortably under

async fn post_body(content_type: &str, len: usize) -> StatusCode {
    make_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", content_type)
                .body(Body::from(vec![b'a'; len]))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn oversize_json_is_rejected() {
    assert_eq!(
        post_body("application/json", OVER_LIMIT).await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a 2 MiB JSON body must be refused — this is the DoS vector"
    );
}

#[tokio::test]
async fn normal_json_passes_through() {
    assert_eq!(
        post_body("application/json", UNDER_LIMIT).await,
        StatusCode::OK,
        "ordinary JSON must be unaffected"
    );
}

#[tokio::test]
async fn oversize_multipart_is_exempt() {
    // The regression that would break every upload in the app. Multipart is
    // governed by the per-handler byte checks (MAX_AVATAR_BYTES, MAX_FPKG_SIZE,
    // …) and the global RequestBodyLimitLayer, not by this middleware.
    assert_eq!(
        post_body("multipart/form-data; boundary=xyz", OVER_LIMIT).await,
        StatusCode::OK,
        "multipart must stay exempt or uploads over 1 MiB break"
    );
}

#[tokio::test]
async fn missing_content_type_is_capped() {
    // No Content-Type is not multipart, so it must be capped — otherwise the
    // cap is opt-out by simply omitting a header.
    assert_eq!(
        post_body("", OVER_LIMIT).await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a bodiless content-type must not buy an exemption"
    );
}

#[tokio::test]
async fn multipart_spoofed_content_type_is_still_bounded_globally() {
    // Documents the accepted residual risk: claiming multipart does bypass THIS
    // layer. That is deliberate (there are 18 multipart handlers across three
    // routers, and enumerating them would silently 413 the next one added), and
    // it is not unbounded — RequestBodyLimitLayer still caps such a request at
    // ~51 MB in the real router, and each upload handler enforces its own limit.
    assert_eq!(
        post_body("multipart/form-data", OVER_LIMIT).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn chunked_oversize_body_is_rejected_without_content_length() {
    // A lying or absent Content-Length must not defeat the cap: the body stream
    // itself is wrapped, so the limit holds on what is actually read.
    let app = make_app();
    let body = Body::from(vec![b'a'; OVER_LIMIT]);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .header("transfer-encoding", "chunked")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::OK,
        "an oversize chunked body must not be read to completion"
    );
}
