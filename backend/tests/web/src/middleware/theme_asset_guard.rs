//! Covers the `/themes` mount being narrowed to `{slug}/assets/**`.
//!
//! A bare `ServeDir` over the themes directory also served `{slug}/templates/*.html`
//! and `{slug}/theme.json`, publishing raw Tera source — context variable names,
//! plugin slot names, internal URL structure — to anonymous callers.
//!
//! The deny cases are the fix. The allow case is the regression guard: get this
//! wrong and every theme's CSS 404s, which breaks the entire site's appearance.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

use ferum_web::routings::theme_asset_guard;

/// Stands in for the `ServeDir` behind the guard: reaching it means the guard
/// allowed the path through.
fn make_app() -> Router {
    Router::new()
        .fallback(get(|| async { "served" }))
        .layer(axum::middleware::from_fn(theme_asset_guard))
}

async fn status_for(path: &str) -> StatusCode {
    make_app()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn serves_theme_assets() {
    // The path shape every template actually requests. If this breaks, so does
    // every page's styling.
    for p in [
        "/default/assets/theme.css",
        "/ferum-review/assets/theme.css",
        "/default/assets/img/nested/logo.png",
    ] {
        assert_eq!(status_for(p).await, StatusCode::OK, "must serve {p}");
    }
}

#[tokio::test]
async fn blocks_template_source() {
    for p in [
        "/default/templates/base.html",
        "/default/templates/partials/nav.html",
        "/ferum-review/templates/home.html",
    ] {
        assert_eq!(
            status_for(p).await,
            StatusCode::NOT_FOUND,
            "{p} leaks Tera source and must 404"
        );
    }
}

#[tokio::test]
async fn blocks_manifest_and_root_listing() {
    for p in ["/default/theme.json", "/default", "/", "/default/"] {
        assert_eq!(status_for(p).await, StatusCode::NOT_FOUND, "must block {p}");
    }
}

#[tokio::test]
async fn blocks_traversal_shaped_slugs() {
    for p in ["/../assets/x.css", "/./assets/x.css"] {
        assert_eq!(
            status_for(p).await,
            StatusCode::NOT_FOUND,
            "must reject traversal-shaped slug in {p}"
        );
    }
}

#[tokio::test]
async fn assets_must_be_the_second_segment() {
    // `assets` deeper in the path is not the assets directory — this is what
    // stops `/default/templates/assets/...` style probing.
    assert_eq!(
        status_for("/default/templates/assets/x.css").await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn tolerates_unstripped_prefix() {
    // `nest_service` strips `/themes`, but the guard accepts both forms so it
    // stays correct if the mount point ever moves.
    assert_eq!(
        status_for("/themes/default/assets/theme.css").await,
        StatusCode::OK
    );
    assert_eq!(
        status_for("/themes/default/templates/base.html").await,
        StatusCode::NOT_FOUND
    );
}
