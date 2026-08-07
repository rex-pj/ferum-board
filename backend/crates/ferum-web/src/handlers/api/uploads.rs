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

/// How long a browser may reuse a `/files/` → object-store redirect.
///
/// The *content* behind a CAS key is immutable; its *location* is not — it moves
/// when the backend, bucket or CDN changes. So this is bounded rather than
/// `immutable`: long enough that a browsing session stops re-asking (which is
/// the entire cost being removed here), short enough that changing backends
/// heals itself within the hour instead of needing every visitor to clear their
/// cache. A permanent redirect would be the wrong tool for the same reason.
const REDIRECT_MAX_AGE_SECS: u32 = 3600;

fn redirect_cache_control() -> String {
    format!("public, max-age={REDIRECT_MAX_AGE_SECS}")
}

/// Rejects keys that could steer a redirect at something other than this key's
/// own object.
///
/// `public_url` builds `{base}/{key}` by concatenation, so a key containing
/// `..` normalises in the browser to a *different* path under the same host —
/// and in the path-style shapes both S3 and GCS use, a different path means a
/// different bucket. Every key `cas_key` emits is `{prefix}/{hex}.{ext}`, so
/// nothing legitimate is excluded.
pub fn is_safe_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with('/')
        && !key.contains("..")
        && !key.contains("//")
        && !key.contains('\\')
        && !key.chars().any(char::is_control)
}

/// Whether a key belongs to the one namespace where a row can be staged.
///
/// Named rather than inlined because three separate decisions depend on it —
/// whether a 304 may be answered from the key alone, whether the database can be
/// skipped, and whether `ref_count == 0` means "staged" — and they must agree.
pub fn may_be_staged(key: &str) -> bool {
    key.starts_with(ATTACHMENT_PREFIX)
}

/// What `/files/` should do with a row, decided before any of it becomes HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDisposition {
    /// Staged, and the caller is not its uploader. 404 rather than 403: whether
    /// a staged attachment exists is not something to confirm to a stranger.
    NotFound,
    /// Staged, caller owns it, bytes are here. Never cacheable.
    StagedBytes,
    /// Staged, caller owns it, bytes are in the object store. Never cacheable.
    StagedRedirect,
    /// Published, bytes are here. Immutable + ETag.
    PublishedBytes,
    /// Published, bytes are in the object store. Bounded cache.
    PublishedRedirect,
}

/// Decides the disposition of a `stored_files` row for a given viewer.
///
/// Pure, and split out of [`serve`] deliberately: the **order** of the two tests
/// inside is security-critical and was once wrong. The staged check used to run
/// after the bytes check, so under an object store — where `data` is always NULL
/// — the redirect fired first and the ownership test was unreachable code. Every
/// staged attachment's location was handed to whoever asked for it.
///
/// A handler needing an `AppState` cannot be exercised in this repository's test
/// suite; this function can, exhaustively, which is the point of it existing.
pub fn resolve_stored_file(
    key: &str,
    ref_count: i32,
    uploaded_by: Option<uuid::Uuid>,
    has_bytes: bool,
    viewer: Option<uuid::Uuid>,
) -> FileDisposition {
    // Staged first. Always.
    if may_be_staged(key) && ref_count <= 0 {
        // `uploaded_by_id` is nullable (ON DELETE SET NULL); a staged row whose
        // owner was deleted is reachable by nobody.
        let is_uploader = matches!((viewer, uploaded_by), (Some(v), Some(o)) if v == o);
        if !is_uploader {
            return FileDisposition::NotFound;
        }
        return if has_bytes {
            FileDisposition::StagedBytes
        } else {
            FileDisposition::StagedRedirect
        };
    }

    if has_bytes {
        FileDisposition::PublishedBytes
    } else {
        FileDisposition::PublishedRedirect
    }
}

/// GET /files/:key — resolve a stored file to its bytes or its location.
///
/// **This endpoint is on the hot path under every backend, not just database
/// storage.** `ports::file_url` is what gets persisted, so avatars, covers,
/// thumbnails, product images, post attachments and site config all point here
/// whatever the bytes are stored in. Under an object store it acts as the
/// indirection that keeps those stored strings valid across a change of bucket
/// or CDN — the same job Facebook's Haystack Directory does.
///
/// Two paths, and the split is deliberate:
///
/// * **Anything outside the attachment namespace** cannot be staged, so its
///   location is a pure function of the key. Under an object store it is
///   redirected with no database work at all.
/// * **Attachments** always go through the database, because a staged one
///   (`ref_count == 0`) is visible only to whoever uploaded it. That lets the
///   composer preview an image before the post exists, while an image that is
///   never posted — or whose post was rejected — is not reachable by anyone
///   else. It cannot be used as anonymous file hosting and it cannot outlive
///   moderation.
///
/// The honest limit on that second guarantee: when the bytes are in a public
/// bucket, this handler can refuse to *tell* a stranger where they are, but it
/// cannot stop someone who already knows the URL. Protection there rests on the
/// key being unguessable and the bucket not being listable.
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
    // A key that could not have come from `cas_key` is refused before it is
    // allowed anywhere near a redirect target.
    if !is_safe_key(&key) {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Staged-ness is a property of this namespace alone, so membership decides
    // which of the two paths below a request takes. Computed once.
    let may_be_staged = may_be_staged(&key);

    if !may_be_staged && crate::utils::if_none_match_hits(&headers, &key) {
        return (StatusCode::NOT_MODIFIED, crate::utils::etag_headers(&key)).into_response();
    }

    // ── Object-store fast path: no database work at all ──────────────────────
    //
    // `public_url` is a pure function of the key, so for a key that cannot be
    // staged there is nothing to look up: whether the row exists changes only
    // *who* answers the 404, and the object store answers it perfectly well.
    //
    // This is the difference between an image costing a connection and costing
    // nothing. Avatars appear once per post and product images once per card, so
    // a listing page was issuing dozens of `stored_files` queries and taking a
    // blob-read permit for each — on a deployment whose entire reason for
    // configuring an object store was to stop serving bytes from this process.
    //
    // A HEAD against the bucket to confirm existence first would undo the point:
    // it trades a local query for an outbound round trip.
    if !may_be_staged {
        let location = state.storage.public_url(&key);
        // Relative means database storage, where `public_url` returns this very
        // path — redirecting to it would loop.
        if location.contains("://") {
            return (
                StatusCode::TEMPORARY_REDIRECT,
                [
                    (header::LOCATION, location),
                    (header::CACHE_CONTROL, redirect_cache_control()),
                ],
            )
                .into_response();
        }
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
            // The decision — including the ordering that makes the staged check
            // effective — lives in `resolve_stored_file`, where it can be tested
            // without an `AppState`. Everything below is marshalling.
            let disposition = resolve_stored_file(
                &file.key,
                file.ref_count,
                file.uploaded_by_id,
                file.data.is_some(),
                auth_user.as_ref().map(|u| u.id),
            );

            // Moved out once, so the two byte-serving arms below bind it rather
            // than each reaching back into `file` for an `Option` they would
            // have to unwrap.
            let data = file.data;

            match disposition {
                FileDisposition::NotFound => StatusCode::NOT_FOUND.into_response(),

                // Bytes are in the object store. This endpoint can decline to
                // tell a stranger where they are; it cannot stop anyone who
                // already has the URL, because the object is world-readable.
                // `private, no-store` because the answer is per viewer — a
                // cacheable redirect would let a shared cache reveal the
                // location to the next person through it.
                FileDisposition::StagedRedirect => (
                    StatusCode::TEMPORARY_REDIRECT,
                    [
                        (header::LOCATION, state.storage.public_url(&file.key)),
                        (header::CACHE_CONTROL, "private, no-store".to_string()),
                    ],
                )
                    .into_response(),

                // A row whose bytes are not here belongs to an external backend.
                // Not an error and not a 404 — the file exists, this endpoint
                // simply is not where it lives.
                FileDisposition::PublishedRedirect => (
                    StatusCode::TEMPORARY_REDIRECT,
                    [
                        (header::LOCATION, state.storage.public_url(&file.key)),
                        (header::CACHE_CONTROL, redirect_cache_control()),
                    ],
                )
                    .into_response(),

                FileDisposition::StagedBytes | FileDisposition::PublishedBytes => {
                    // `resolve_stored_file` returns these two only when told the
                    // row has bytes, so this cannot fire. It is a hard error
                    // rather than an empty body on purpose: a 200 carrying zero
                    // bytes with an image content-type is a corrupt file that
                    // looks like a successful response, and it would be blamed
                    // on the uploader or the browser long before anyone looked
                    // here.
                    let Some(bytes) = data else {
                        tracing::error!(
                            key = %file.key,
                            "stored_files row classified as having bytes but `data` is NULL"
                        );
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    };

                    let staged = disposition == FileDisposition::StagedBytes;
                    // Published files are content-addressed and immutable, so
                    // they may be cached forever and revalidated by ETag. A
                    // staged one is authorized per viewer and gets neither.
                    let cache_control = if staged {
                        "private, no-store".to_string()
                    } else {
                        "public, max-age=31536000, immutable".to_string()
                    };
                    let mut headers = vec![
                        (header::CONTENT_TYPE, file.content_type.clone()),
                        (header::CACHE_CONTROL, cache_control),
                        (header::CONTENT_DISPOSITION, "attachment".to_string()),
                    ];
                    if !staged {
                        // Without this the 304 short-circuit above can never
                        // fire: a client only sends `If-None-Match` for an ETag
                        // it was given. `immutable` already suppresses
                        // revalidation in browsers that honour it, but shared
                        // caches, `no-cache` reloads and non-browser clients all
                        // revalidate anyway, and those are exactly the requests
                        // worth answering without a database round trip.
                        headers.push((header::ETAG, crate::utils::etag_for(&file.key)));
                    }

                    // `AppendHeaders` rather than an array literal: the two arms
                    // differ only by the ETag, and a `Vec` keeps that one
                    // difference expressed once instead of duplicating the
                    // whole header set per arm.
                    (
                        StatusCode::OK,
                        axum::response::AppendHeaders(headers),
                        bytes,
                    )
                        .into_response()
                }
            }
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
