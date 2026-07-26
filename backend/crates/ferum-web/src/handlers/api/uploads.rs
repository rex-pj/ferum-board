use std::path::PathBuf;

use axum::extract::{Extension, Path, State};
use axum::http::{header, StatusCode};
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
    Path(key): Path<String>,
) -> Response {
    let result = stored_files::Entity::find_by_id(&key).one(&state.db).await;

    match result {
        Ok(Some(file)) => {
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
                    file.data,
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
                    (header::CONTENT_DISPOSITION, "attachment".to_string()),
                ],
                file.data,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
