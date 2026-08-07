//! Unit tests for [`S3StorageService`]'s URL handling.
//!
//! # Why this file exists now
//!
//! `key_from_url` was rewritten to share `storage/url_shapes.rs` with the GCS
//! adapter, and that rewrite changed behaviour: a presigned URL now resolves,
//! where before the query string prevented a match. Nothing in the suite
//! observed either the old or the new behaviour — this adapter had no tests at
//! all — so the change went in unwatched. These pin it.
//!
//! The failure mode being guarded is silent. `key_from_url` returning `None`
//! for a URL we minted is not an error; it reports "not one of ours", so a CAS
//! reference is never taken or never released, and files are either leaked or
//! garbage collected out from under posts that still display them.
//!
//! # Scope
//!
//! Offline only. `put` and `delete` issue real S3 requests and are not covered
//! here or anywhere else — same honest limitation as the GCS adapter. The
//! constructor performs no network I/O because credentials, region and endpoint
//! are all supplied explicitly, which is what lets these run without a bucket.

use ferum_application::ports::StorageService;
use ferum_infrastructure::storage::S3StorageService;

const ENDPOINT: &str = "http://minio:9000";
const BUCKET: &str = "forum-uploads";

/// `cdn` mirrors `CDN_BASE_URL`: `Some` when configured, `None` when not.
async fn service(cdn: Option<&str>) -> S3StorageService {
    S3StorageService::new(ENDPOINT, "access", "secret", BUCKET, "us-east-1", cdn).await
}

/// The common case in these tests: a CDN is configured.
async fn with_cdn() -> S3StorageService {
    service(Some("https://cdn.example.com")).await
}

// ─── The contract: public_url and key_from_url are inverses ───────────────────

#[tokio::test]
async fn public_url_round_trips() {
    let svc = with_cdn().await;
    let url = svc.public_url("avatars/abc123.png");
    assert_eq!(url, "https://cdn.example.com/avatars/abc123.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/abc123.png"));
}

#[tokio::test]
async fn a_trailing_slash_on_the_cdn_base_does_not_double_up() {
    let svc = service(Some("https://cdn.example.com/")).await;
    assert_eq!(svc.public_url("k.png"), "https://cdn.example.com/k.png");
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/k.png").as_deref(),
        Some("k.png")
    );
}

// ─── No CDN: the URL must carry the bucket ────────────────────────────────────
//
// `startup.rs` used to pass the endpoint itself as the CDN base when
// `CDN_BASE_URL` was unset, so `public_url` emitted `{endpoint}/{key}` — an
// address where nothing lives, because the object is at
// `{endpoint}/{bucket}/{key}`. Every avatar, logo and post image written by such
// a deployment was a broken link, and nothing in the app would ever say so.

#[tokio::test]
async fn public_url_includes_the_bucket_when_no_cdn_is_configured() {
    let svc = service(None).await;
    let url = svc.public_url("avatars/a.png");
    assert_eq!(
        url, "http://minio:9000/forum-uploads/avatars/a.png",
        "the bucket segment is what makes the object actually resolvable"
    );
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

/// The bucket-less shape is **not** recognised, deliberately.
///
/// Accepting it would mean carrying the pre-fix URL shape forever. There is no
/// such data to protect — a URL in that form addresses nothing, so treating it
/// as foreign is both correct and the narrower contract.
#[tokio::test]
async fn bucket_less_urls_are_not_claimed() {
    let svc = service(None).await;
    assert_eq!(svc.key_from_url("http://minio:9000/avatars/a.png"), None);
}

/// An operator who points `CDN_BASE_URL` straight at the S3 endpoint makes it a
/// strict prefix of `{endpoint}/{bucket}`, which is the one way the two accepted
/// bases can still overlap. Longest-first matching is what resolves it.
#[tokio::test]
async fn a_cdn_base_equal_to_the_endpoint_does_not_swallow_the_bucket() {
    let svc = service(Some(ENDPOINT)).await;
    assert_eq!(
        svc.key_from_url("http://minio:9000/forum-uploads/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png"),
        "must not resolve to `forum-uploads/avatars/a.png`, a key that exists nowhere"
    );
}

/// Adding `CDN_BASE_URL` to a running forum must not orphan what it wrote
/// before. This is ordinary operation, not legacy data: the URLs are the ones
/// this adapter mints today, and they stay in post HTML after the switch.
#[tokio::test]
async fn a_cdn_configured_backend_still_reads_direct_endpoint_urls() {
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url("http://minio:9000/forum-uploads/avatars/old.png")
            .as_deref(),
        Some("avatars/old.png")
    );
}

// ─── The legacy same-origin form ──────────────────────────────────────────────
//
// The migration case: an install that ran on database storage has `/files/{key}`
// baked into user rows, site config and the stored HTML of every post. Those
// strings are never rewritten, and the rows keep their bytes, so they have to
// stay resolvable after the switch to S3.

#[tokio::test]
async fn legacy_same_origin_urls_are_still_recognised() {
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url("/files/avatars/old.png").as_deref(),
        Some("avatars/old.png")
    );
}

#[tokio::test]
async fn legacy_cdn_prefixed_urls_are_still_recognised() {
    // Written by `DatabaseStorageService::public_url` while CDN_BASE_URL was
    // set. Ambiguous with the bare `{cdn}/{key}` shape, which is why the legacy
    // rule is checked first — see the ordering in `key_from_url`.
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/files/avatars/old.png")
            .as_deref(),
        Some("avatars/old.png"),
        "the legacy /files/ segment must win over the bare {{cdn}}/{{key}} shape"
    );
}

// ─── Query strings ────────────────────────────────────────────────────────────

#[tokio::test]
async fn presigned_urls_resolve_to_the_same_key() {
    // BEHAVIOUR CHANGE, introduced with the `url_shapes` refactor: the
    // authorization lives in the query string and is no part of the key. Before,
    // the trailing `?X-Amz-…` made the whole URL unrecognisable, so a presigned
    // URL stored in a post held no reference to its own file.
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url(
            "https://cdn.example.com/avatars/a.png\
             ?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Expires=900&X-Amz-Signature=deadbeef"
        )
        .as_deref(),
        Some("avatars/a.png")
    );
}

#[tokio::test]
async fn a_cache_buster_does_not_change_the_key() {
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/avatars/a.png?v=2").as_deref(),
        Some("avatars/a.png")
    );
}

#[tokio::test]
async fn a_fragment_is_not_part_of_the_key() {
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/avatars/a.png#top").as_deref(),
        Some("avatars/a.png")
    );
}

// ─── URLs that must NOT be claimed ────────────────────────────────────────────

#[tokio::test]
async fn foreign_urls_are_not_claimed() {
    // An author may paste any external image into a post; claiming one would
    // decrement a reference count that belongs to nothing.
    let svc = with_cdn().await;
    assert_eq!(svc.key_from_url("https://example.com/photo.png"), None);
}

#[tokio::test]
async fn a_cdn_host_that_merely_starts_with_ours_is_not_claimed() {
    let svc = with_cdn().await;
    assert_eq!(
        svc.key_from_url("https://cdn.example.com.evil.test/avatars/a.png"),
        None,
        "the separator after the base must be a `/`, not any character"
    );
}

#[tokio::test]
async fn an_empty_key_is_not_a_key() {
    let svc = with_cdn().await;
    assert_eq!(svc.key_from_url("/files/"), None);
    assert_eq!(svc.key_from_url("https://cdn.example.com/"), None);
}
