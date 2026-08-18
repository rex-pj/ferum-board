//! `/files/` blob serving and upload endpoints.
//!
//! See [`resolve_stored_file`] for the ordering rule that keeps staged
//! attachments private under an object store, and [`Viewer`] for who counts as
//! staff — the one category of viewer besides the uploader that may read one.

use std::path::PathBuf;

use axum::extract::{Extension, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::EntityTrait;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use ferum_domain::models::role::perm;
use ferum_infrastructure::entities::stored_files;

/// GET /plugins/:slug/assets/*path — serve a plugin UI asset.
///
/// `/assets/` is a URL namespace only: `.fpkg` archives are flat, and the same
/// bundle.js is what boa_engine loads via `{install_path}/{bundle_file}`. Both
/// consumers therefore resolve against the same flat root.
///
/// `plugin.toml` is blocked, and `..` in either segment returns 404.
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

// `std::path::Path` spelled out: this module imports `axum::extract::Path`.
fn content_type_for_path(path: &std::path::Path) -> &'static str {
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

/// Who is asking, reduced to the two things a staged row's disposition turns on.
///
/// A struct rather than two positional arguments because `resolve_stored_file`
/// already takes an `Option<Uuid>` and a `bool`, and a second pair of those in a
/// six-argument call is a transposition waiting to happen — on the one function
/// here where a transposition publishes a private file.
///
/// **The fields are private and there is no way to set `is_staff` except
/// [`Viewer::from_auth`].** They were briefly `pub`, which made
/// `Viewer { id: None, is_staff: true }` writable at any call site — on the one
/// type whose entire job is gating access to private files. Staff-ness is derived
/// from a resolved permission set or it is not derived at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Viewer {
    id: Option<uuid::Uuid>,
    /// Cleared for guests and for banned accounts.
    /// See [`Viewer::from_auth`] for which permissions set it.
    is_staff: bool,
}

impl Viewer {
    /// Nobody signed in.
    pub fn guest() -> Self {
        Self::default()
    }

    /// A signed-in member with no moderation powers.
    pub fn member(id: uuid::Uuid) -> Self {
        Self {
            id: Some(id),
            is_staff: false,
        }
    }

    pub fn id(&self) -> Option<uuid::Uuid> {
        self.id
    }

    pub fn is_staff(&self) -> bool {
        self.is_staff
    }

    /// Reads staff-ness off the resolved permission set.
    ///
    /// `moderation.view_reports` **in any category** — not the global-only
    /// `has_perm` — because a category-scoped moderator is the normal shape of
    /// the role, and this endpoint holds only a CAS key: nothing in
    /// `post-attachments/{hash}.jpg` names the category the post lived in, and
    /// recovering it would mean a `content_md LIKE` scan on the hot path.
    ///
    /// Widening from "this moderator's categories" to "any" is acceptable
    /// because the key is itself the capability — 128 bits of digest, not
    /// enumerable, and only ever handed out by surfaces that are already
    /// permission-gated (the report queue, the mod log, `/admin/storage`).
    ///
    /// `admin.config` is accepted too, so whoever can open `/admin/storage` can
    /// see the thumbnails on it. Stock `admin` holds both; a custom role need
    /// only hold one.
    /// A **banned** account holds no powers, so `is_staff` is cleared before any
    /// permission is consulted — the same "ban check first" order every use case
    /// follows, which this endpoint previously skipped entirely. Their own id is
    /// kept: reading back a file they uploaded themselves is not a power, and
    /// revoking it would break nothing an abuser can exploit.
    pub fn from_auth(auth: Option<&AuthUser>) -> Self {
        let Some(user) = auth else {
            return Self::guest();
        };
        Self {
            id: Some(user.id),
            is_staff: !user.is_currently_banned()
                && (user.has_perm_any_category(perm::MOD_VIEW_REPORTS)
                    || user.has_perm(perm::ADMIN_CONFIG)),
        }
    }
}

/// What `/files/` should do with a row, decided before any of it becomes HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDisposition {
    /// Staged, and the caller is neither its uploader nor staff. 404 rather than
    /// 403: whether a staged attachment exists is not something to confirm to a
    /// stranger.
    NotFound,
    /// Staged, caller may see it, bytes are here. Never cacheable.
    StagedBytes,
    /// Staged, caller may see it, bytes are in the object store. Never cacheable.
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
///
/// **Staff see staged attachments, and that is the point rather than a
/// loosening.** `ref_count` reaching zero is also what soft-deleting a post does
/// to its images, and those objects are deliberately kept as the evidence for the
/// removal — when the image *is* the violation, it is the whole case. While the
/// only accepted viewer was the uploader, that evidence was readable by the
/// account that posted it and by nobody else, including the moderator who removed
/// the post.
pub fn resolve_stored_file(
    key: &str,
    ref_count: i32,
    uploaded_by: Option<uuid::Uuid>,
    has_bytes: bool,
    viewer: Viewer,
) -> FileDisposition {
    // Staged first. Always.
    if may_be_staged(key) && ref_count <= 0 {
        // `uploaded_by_id` is nullable (ON DELETE SET NULL); a staged row whose
        // owner was deleted is reachable by no member — but staff still reach it,
        // which is exactly the case a deleted account's abandoned upload creates.
        let is_uploader = matches!((viewer.id, uploaded_by), (Some(v), Some(o)) if v == o);
        if !is_uploader && !viewer.is_staff {
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

/// How long a response for a given disposition may be kept, and by whom.
///
/// Split out for the same reason as [`resolve_stored_file`]: `serve` needs an
/// `AppState` and cannot be constructed in this repository's test suite, so while
/// these strings were literals inside its match arms the rule that a *staged*
/// response is never cacheable had nothing guarding it. That rule is what stops a
/// shared cache handing a removed post's image to the next person through it, and
/// it now matters to more viewers than it used to — [`Viewer`] admits staff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePolicy {
    pub cache_control: String,
    /// Whether the response may carry an ETag.
    ///
    /// Only ever true where `cache_control` permits storing the response: an ETag
    /// invites the revalidation that the 304 short-circuit at the top of [`serve`]
    /// answers from the key alone, and that short-circuit deliberately never runs
    /// for a staged key.
    pub etag: bool,
}

/// The caching each disposition is allowed. `None` for [`FileDisposition::NotFound`]
/// — there is no body, so there is nothing to cache.
///
/// One authority for all three places that answer a `/files/` request: both
/// redirect arms, the byte-serving arm, and the object-store fast path that skips
/// the database entirely.
pub fn cache_policy(disposition: FileDisposition) -> Option<CachePolicy> {
    let policy = match disposition {
        FileDisposition::NotFound => return None,
        // Authorized per viewer, so the answer is not shared. `no-store` rather
        // than `private`+`max-age=0`: a staged attachment must not sit in a disk
        // cache either.
        FileDisposition::StagedBytes | FileDisposition::StagedRedirect => CachePolicy {
            cache_control: "private, no-store".to_string(),
            etag: false,
        },
        // A location moves when the backend, bucket or CDN changes, so this is
        // bounded rather than `immutable` — see `REDIRECT_MAX_AGE_SECS`.
        FileDisposition::PublishedRedirect => CachePolicy {
            cache_control: redirect_cache_control(),
            etag: false,
        },
        // Content-addressed and public: the bytes behind this key can never
        // change, so it may be cached forever and revalidated by ETag.
        FileDisposition::PublishedBytes => CachePolicy {
            cache_control: "public, max-age=31536000, immutable".to_string(),
            etag: true,
        },
    };
    Some(policy)
}

/// GET /files/:key — resolves a stored file to its bytes or its location.
///
/// **Hot path under every backend**, since `ports::file_url` is what gets
/// persisted. Two paths: keys outside the attachment namespace cannot be staged,
/// so they redirect with no database work; attachments always hit the database,
/// because a staged one (`ref_count == 0`) is visible only to its uploader.
///
/// Once bytes are in a public bucket that second guarantee rests on the key
/// being unguessable and the bucket not listable — not on this handler.
pub async fn serve(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    // Revalidate before touching the database: a CAS key is a digest of its own
    // bytes, so it IS a strong ETag and matching `If-None-Match` needs no row.
    // Otherwise every cold client costs a full `bytea` read on the pool that
    // renders pages.
    //
    // Staged attachments MUST stay excluded — they are authorized per viewer
    // below, so a 304 here would answer what the owner check must refuse.
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
    // `public_url` is a pure function of the key, so a key that cannot be staged
    // needs no lookup — the row's existence only changes who answers the 404.
    // Without this a listing page issues dozens of `stored_files` queries and
    // takes a blob-read permit each, on a deployment configured precisely to
    // stop serving bytes here. A HEAD to confirm existence would undo the point.
    if !may_be_staged {
        let location = state.storage.public_url(&key);
        // Relative means database storage, where `public_url` returns this very
        // path — redirecting to it would loop.
        if location.contains("://") {
            // A key that cannot be staged is by definition published, so this
            // reads the same policy the database path would have reached. Sharing
            // it is the point: a second literal here is how the fast path and the
            // slow path come to disagree about caching for the same key.
            let cache = cache_policy(FileDisposition::PublishedRedirect)
                .expect("PublishedRedirect always has a policy");
            return (
                StatusCode::TEMPORARY_REDIRECT,
                [
                    (header::LOCATION, location),
                    (header::CACHE_CONTROL, cache.cache_control),
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
                Viewer::from_auth(auth_user.as_ref()),
            );

            // Moved out once, so the two byte-serving arms below bind it rather
            // than each reaching back into `file` for an `Option` they would
            // have to unwrap.
            let data = file.data;

            // Every arm below reads its caching from here rather than spelling it
            // out, so the "staged is never cacheable" rule lives in one testable
            // place. `None` is exactly the 404 arm.
            let Some(cache) = cache_policy(disposition) else {
                return StatusCode::NOT_FOUND.into_response();
            };

            match disposition {
                // Already handled by the `else` above, which is the only path a
                // refusal takes. Kept rather than collapsed into a `_` arm: this
                // match is what forces a new `FileDisposition` variant to be given
                // an HTTP meaning here, and a catch-all would silently hand one
                // the byte-serving branch. Redundant by construction and identical
                // to the live path either way, so the duplication costs nothing
                // that the exhaustiveness check does not repay.
                FileDisposition::NotFound => StatusCode::NOT_FOUND.into_response(),

                // Bytes are in the object store. This endpoint can decline to
                // tell a stranger where they are; it cannot stop anyone who
                // already has the URL, because the object is world-readable. The
                // redirect is uncacheable because the answer is per viewer — a
                // shared cache would otherwise reveal the location to the next
                // person through it.
                //
                // A row whose bytes are not here belongs to an external backend:
                // not an error and not a 404, the file exists and this endpoint
                // simply is not where it lives. The two differ only in caching,
                // which `cache_policy` already decided.
                FileDisposition::StagedRedirect | FileDisposition::PublishedRedirect => (
                    StatusCode::TEMPORARY_REDIRECT,
                    [
                        (header::LOCATION, state.storage.public_url(&file.key)),
                        (header::CACHE_CONTROL, cache.cache_control),
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

                    // The revalidation the ETag invites, answered here rather than
                    // only at the top of this function.
                    //
                    // That early short-circuit is gated on `!may_be_staged`,
                    // because staged-ness cannot be known without the row — which
                    // meant a *published* `post-attachments/` key advertised an
                    // ETag nothing ever compared: the client sent `If-None-Match`
                    // and got 200 with the whole image every time. `cache.etag` is
                    // the same flag that decides whether the header goes out below,
                    // so the promise and the honouring cannot diverge.
                    //
                    // **What this saves is the response body, not the query.** The
                    // row above is already fetched, blob included, and the permit is
                    // already held. Deciding earlier would need a metadata-only
                    // query first — and that is a bad trade, not an unfinished one:
                    // under database storage (the default) every attachment byte
                    // serve would pay a second round trip, while the hydration it
                    // avoids only ever happens on a revalidation of an attachment.
                    // Under an object store there is nothing to hydrate anyway,
                    // since promoted rows carry `data IS NULL`.
                    if cache.etag && crate::utils::if_none_match_hits(&headers, &file.key) {
                        return (
                            StatusCode::NOT_MODIFIED,
                            crate::utils::etag_headers(&file.key),
                        )
                            .into_response();
                    }

                    // Named apart from the `headers` request map above, which the
                    // revalidation check reads. One name for both invited a
                    // misreading of which side of the shadow a use sat on.
                    let mut out_headers = vec![
                        (header::CONTENT_TYPE, file.content_type.clone()),
                        (header::CACHE_CONTROL, cache.cache_control),
                        (header::CONTENT_DISPOSITION, "attachment".to_string()),
                    ];
                    if cache.etag {
                        // Without this the 304 short-circuit above can never
                        // fire: a client only sends `If-None-Match` for an ETag
                        // it was given. `immutable` already suppresses
                        // revalidation in browsers that honour it, but shared
                        // caches, `no-cache` reloads and non-browser clients all
                        // revalidate anyway, and those are exactly the requests
                        // worth answering without a database round trip.
                        //
                        // Never for a staged row: `cache_policy` clears this flag
                        // there, so the two decisions cannot drift apart.
                        out_headers.push((header::ETAG, crate::utils::etag_for(&file.key)));
                    }

                    // `AppendHeaders` rather than an array literal: the two arms
                    // differ only by the ETag, and a `Vec` keeps that one
                    // difference expressed once instead of duplicating the
                    // whole header set per arm.
                    (
                        StatusCode::OK,
                        axum::response::AppendHeaders(out_headers),
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
