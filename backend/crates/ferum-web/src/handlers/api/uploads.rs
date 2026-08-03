use std::path::PathBuf;

use axum::extract::{Extension, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::EntityTrait;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use ferum_infrastructure::entities::stored_files;

/// GET /plugins/:slug/assets/*path — serve a plugin UI asset.
///
/// `/assets/` is a URL-only namespace, not a required on-disk subfolder: a
/// plugin's `.fpkg` is a flat archive (`plugin.toml` + `bundle.js` at the
/// package root — see every example under examples/plugins/), and the SAME
/// bundle.js is what `[script].bundle_file` in the manifest points
/// boa_engine at via `{install_path}/{bundle_file}` (also flat, no `assets/`
/// prefix — see ScriptPluginRuntime::load_bundle). This route resolves
/// against the same flat install root so both consumers agree on where the
/// file actually lives, and blocks serving plugin.toml so the manifest
/// itself isn't reachable through the same public path.
/// Path traversal attempts (`..`) in either segment return 404.
pub async fn serve_plugin_asset(
    State(state): State<AppState>,
    Path((slug, asset_path)): Path<(String, String)>,
) -> Response {
    // Reject any traversal attempt in slug or path
    if slug.contains("..") || slug.contains('/') || slug.contains('\\')
        || asset_path.contains("..")
        || asset_path.eq_ignore_ascii_case("plugin.toml")
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    // `..` is not the only way out of the base directory: `Path::join` REPLACES
    // the base entirely when its argument is absolute or carries a Windows drive
    // prefix, so `/plugins/x/assets/C:/Windows/win.ini` would escape without ever
    // containing `..`. Reject those components explicitly — the same check the
    // plugin package extractor already applies on the way in.
    let rel = std::path::Path::new(&asset_path);
    if rel.components().any(|c| {
        matches!(
            c,
            std::path::Component::RootDir
                | std::path::Component::ParentDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let plugin_root = PathBuf::from(&state.plugins_dir).join(&slug);
    let file_path = plugin_root.join(rel);

    // Belt-and-braces: even with the component filter above, confirm the resolved
    // path really is inside this plugin's directory before reading it. Compares
    // canonical forms so a symlink inside the package cannot point outward.
    match (
        tokio::fs::canonicalize(&plugin_root).await,
        tokio::fs::canonicalize(&file_path).await,
    ) {
        (Ok(root), Ok(resolved)) if resolved.starts_with(&root) => {}
        _ => return StatusCode::NOT_FOUND.into_response(),
    }

    match tokio::fs::read(&file_path).await {
        Ok(data) => {
            let ct = content_type_for_path(&file_path);
            // security_headers middleware adds nosniff/frame-options/CSP to all responses.
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, ct.to_string()),
                    // Unlike CAS-stored files (content-addressed by hash — safe to
                    // cache forever), a plugin's bundle.js can change any time an
                    // admin reinstalls/updates the plugin while keeping the same
                    // URL. No ETag/Last-Modified is served, so a long max-age would
                    // leave browsers showing a stale bundle for up to an hour with
                    // no way to detect the change.
                    (header::CACHE_CONTROL, "no-cache".to_string()),
                ],
                data,
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type_for_path(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("js") | Some("mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// CAS namespace for images embedded in post content. Files here are uploaded
/// before the post that references them exists, so they start life *staged*
/// (`ref_count == 0`) and are not public until some post embeds them.
const ATTACHMENT_PREFIX: &str = "post-attachments/";

/// GET /files/:key — serve a file stored in the database.
/// This endpoint is only reached when DatabaseStorageService is active (no S3).
///
/// Staged post attachments are visible only to whoever uploaded them, so the
/// composer can preview an image before the post is submitted, while an image
/// that is never posted (or whose post was rejected/deleted) is not reachable
/// by anyone else — it cannot be used as anonymous file hosting, and it cannot
/// outlive moderation.
///
/// NOTE: this gate lives in the DB-storage path. Under `S3StorageService`,
/// blobs are served straight from S3/CDN and never reach this handler, so
/// staging is not enforced there.
pub async fn serve(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    // Answer revalidation before touching the database.
    //
    // A CAS key is a digest of the bytes it names, so it *is* a strong ETag:
    // the content behind a key can never change, which is what makes matching
    // the key against `If-None-Match` sound without reading the row first. That
    // is the entire point — this handler otherwise loads the whole blob into
    // memory through the write pool, so every cold client and every CDN miss
    // costs a full `bytea` read on the same 20 connections that render pages.
    //
    // Staged attachments are excluded, and must stay excluded: they are
    // authorized per viewer below, so a 304 here would answer a request that
    // the owner check should have refused. A key outside that namespace can
    // never be staged, so the exclusion is exact rather than conservative.
    if !key.starts_with(ATTACHMENT_PREFIX) && crate::utils::if_none_match_hits(&headers, &key) {
        return (StatusCode::NOT_MODIFIED, crate::utils::etag_headers(&key)).into_response();
    }

    // Past this point the request costs a connection and a full copy of the
    // file in memory, so it queues behind a fixed number of peers rather than
    // competing freely for the pool. Held for the query only — the response
    // body is already in memory by then, so a slow client cannot pin a permit.
    let Ok(_permit) = state.blob_read_permits.clone().acquire_owned().await else {
        // Only reachable if the semaphore was closed, which nothing does.
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    // Read pool: blob traffic has no reason to contend with posting, and where
    // a replica is configured this moves it off the primary entirely.
    let result = stored_files::Entity::find_by_id(&key).one(&state.db_read).await;

    match result {
        Ok(Some(file)) => {
            // A row whose bytes are not here belongs to an external backend. It
            // is not an error and not a 404 either — the file exists, this
            // endpoint simply is not where it lives. Redirecting keeps every URL
            // this application has ever minted resolvable, including the
            // `/files/...` strings baked into years of post HTML, without the
            // app having to proxy the bytes it deliberately moved off itself.
            let Some(bytes) = file.data else {
                return (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(header::LOCATION, state.storage.public_url(&file.key))],
                )
                    .into_response();
            };

            let is_staged_attachment =
                file.key.starts_with(ATTACHMENT_PREFIX) && file.ref_count <= 0;

            if is_staged_attachment {
                // `uploaded_by_id` is nullable (ON DELETE SET NULL); a staged row
                // with no owner is reachable by nobody.
                let is_uploader = match (auth_user.as_ref(), file.uploaded_by_id) {
                    (Some(u), Some(owner)) => u.id == owner,
                    _ => false,
                };
                if !is_uploader {
                    return StatusCode::NOT_FOUND.into_response();
                }

                // Must not land in a shared cache: this response is authorized
                // per-viewer, unlike every published (ref_count > 0) file, whose
                // content-addressed key makes it safe to cache forever.
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, file.content_type.clone()),
                        (header::CACHE_CONTROL, "private, no-store".to_string()),
                        (header::CONTENT_DISPOSITION, "attachment".to_string()),
                    ],
                    bytes,
                )
                    .into_response();
            }

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, file.content_type.clone()),
                    (
                        header::CACHE_CONTROL,
                        "public, max-age=31536000, immutable".to_string(),
                    ),
                    // Without this the short-circuit above can never fire: a
                    // client only sends `If-None-Match` for an ETag it was
                    // given. `immutable` already suppresses revalidation in
                    // browsers that honour it, but shared caches, `no-cache`
                    // reloads and non-browser clients all revalidate anyway,
                    // and those are exactly the requests worth answering
                    // without a database round trip.
                    (header::ETAG, crate::utils::etag_for(&file.key)),
                    (header::CONTENT_DISPOSITION, "attachment".to_string()),
                ],
                bytes,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
