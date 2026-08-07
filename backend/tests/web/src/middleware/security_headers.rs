use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use ferum_web::middleware::security_headers::{
    csp_origin_of, security_headers, SecurityHeadersConfig,
};

/// `https_enabled = false` mirrors a plain-HTTP dev deployment, which is what
/// every pre-existing assertion below was written against.
fn make_app() -> Router {
    make_app_with_https(false)
}

fn make_app_with_https(https_enabled: bool) -> Router {
    make_app_with(https_enabled, &[])
}

/// `image_origins` mirrors what `startup.rs` derives from
/// `StorageService::public_url` — empty for a same-origin install.
fn make_app_with(https_enabled: bool, image_origins: &[String]) -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            SecurityHeadersConfig::new(https_enabled, image_origins),
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

// ─── img-src and the storage backend ─────────────────────────────────────────
//
// `img-src 'self' data: blob:` was a constant, and it silently broke every
// deployment whose uploads are not same-origin: S3, GCS, and plain database
// storage behind CDN_BASE_URL alike. The upload succeeds, the row is written,
// `public_url` returns the right address, and the browser then refuses to load
// it — no server error, nothing in the logs, just a page of blank images and a
// console violation. `startup.rs` now derives the origin from
// `StorageService::public_url` and passes it here.

#[tokio::test]
async fn an_object_store_origin_is_added_to_img_src() {
    let origins = vec!["https://storage.googleapis.com".to_string()];
    let resp = make_app_with(false, &origins)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let csp = resp.headers()["content-security-policy"].to_str().unwrap();
    assert!(
        csp.contains("img-src 'self' data: blob: https://storage.googleapis.com"),
        "uploaded images are unreachable without this: {csp}"
    );
}

#[tokio::test]
async fn widening_img_src_does_not_widen_script_or_connect_src() {
    // An object store holds user-uploaded bytes. Letting a bucket serve scripts
    // to this origin would promote an upload-validation bug into code execution,
    // so only img-src moves.
    let origins = vec!["https://cdn.example.com".to_string()];
    let resp = make_app_with(false, &origins)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let csp = resp.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("script-src 'self' 'unsafe-eval';"), "{csp}");
    assert!(csp.contains("connect-src 'self';"), "{csp}");
    assert!(csp.contains("default-src 'self';"), "{csp}");
}

#[tokio::test]
async fn a_same_origin_install_keeps_the_strict_policy() {
    // Database storage without a CDN mints `/files/{key}`, which is already
    // covered by 'self'. Nothing should be appended.
    let resp = make_app_with(false, &[])
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let csp = resp.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("img-src 'self' data: blob:;"), "{csp}");
}

#[test]
fn csp_origin_of_extracts_scheme_and_authority() {
    assert_eq!(
        csp_origin_of("https://storage.googleapis.com/bucket/avatars/a.png").as_deref(),
        Some("https://storage.googleapis.com")
    );
    assert_eq!(
        csp_origin_of("http://minio:9000/forum-uploads/a.png").as_deref(),
        Some("http://minio:9000"),
        "a port is part of the origin and must survive"
    );
}

#[test]
fn a_relative_public_url_yields_no_origin() {
    // `DatabaseStorageService` without a CDN returns `/files/{key}`; there is
    // nothing to add, and inventing an entry would widen the policy for nothing.
    assert_eq!(csp_origin_of("/files/avatars/a.png"), None);
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
