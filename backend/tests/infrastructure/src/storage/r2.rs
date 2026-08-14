//! Unit tests for [`R2StorageService`]'s URL handling and constructor rules.
//!
//! # What is actually at stake here
//!
//! Two different silent failures, and this adapter exists to make both loud.
//!
//! `key_from_url` returning `None` for a URL we minted is not an error — it
//! reports "not one of ours", so a CAS reference is never taken or never
//! released, and files are either leaked or garbage collected out from under
//! posts still displaying them. Claiming a URL that is *not* ours fails the same
//! way in reverse. R2 is served under more shapes than S3 is (the public origin
//! is a different host from the API endpoint), so the accept/reject boundary
//! carries more weight.
//!
//! The constructor rules guard the worse one. `public_url` is written into
//! `users.avatar_url`, `site_config` and the stored HTML of every post, and
//! those strings are never rewritten — so a configuration that mints signed-only
//! URLs does not degrade, it writes permanently-broken links into content that
//! cannot be repaired without a data migration. Every rejection below is
//! preventing exactly that.
//!
//! # Scope
//!
//! Offline only. `put` and `delete` issue real requests and are not covered here
//! or anywhere else — the same honest limitation as the S3 and GCS adapters. The
//! constructor performs no network I/O, which is what lets these run without a
//! bucket.

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;
use ferum_infrastructure::storage::R2StorageService;

const ACCOUNT: &str = "abc123def456";
const BUCKET: &str = "forum-uploads";
const PUBLIC_BASE: &str = "https://cdn.example.com";
const API_ROOT: &str = "https://abc123def456.r2.cloudflarestorage.com";

async fn service() -> R2StorageService {
    build(Some(PUBLIC_BASE), None).await.expect("valid config")
}

async fn build(
    public_base: Option<&str>,
    cdn: Option<&str>,
) -> Result<R2StorageService, AppError> {
    R2StorageService::new(ACCOUNT, None, BUCKET, "access", "secret", public_base, cdn).await
}

/// `Result::expect_err` needs `T: Debug`, and the service does not derive it.
fn construction_error(result: Result<R2StorageService, AppError>, why: &str) -> String {
    match result {
        Ok(_) => panic!("expected construction to fail: {why}"),
        Err(e) => e.to_string(),
    }
}

// ─── The contract: public_url and key_from_url are inverses ───────────────────

#[tokio::test]
async fn public_url_round_trips() {
    let svc = service().await;
    let url = svc.public_url("avatars/abc123.png");
    assert_eq!(url, "https://cdn.example.com/avatars/abc123.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/abc123.png"));
}

#[tokio::test]
async fn public_url_never_mints_the_signed_only_api_endpoint() {
    // The single most important assertion in this file. Every other backend can
    // fall back to its vendor host; R2's refuses anonymous reads, so a URL under
    // it in post HTML is a permanently broken image.
    let svc = service().await;
    let url = svc.public_url("avatars/abc123.png");
    assert!(
        !url.contains("r2.cloudflarestorage.com"),
        "minted the signed-only API endpoint: {url}"
    );
}

#[tokio::test]
async fn a_trailing_slash_on_the_public_base_does_not_double_up() {
    let svc = build(Some("https://cdn.example.com/"), None)
        .await
        .expect("valid config");
    let url = svc.public_url("avatars/a.png");
    assert_eq!(url, "https://cdn.example.com/avatars/a.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

#[tokio::test]
async fn an_r2_dev_public_base_round_trips() {
    // Warned about at construction, but it must still work — it is what a
    // development install has before anyone binds a domain.
    let svc = build(Some("https://pub-0123456789abcdef.r2.dev"), None)
        .await
        .expect("r2.dev is accepted, with a warning");
    let url = svc.public_url("avatars/a.png");
    assert_eq!(url, "https://pub-0123456789abcdef.r2.dev/avatars/a.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

// ─── Shapes minted by whatever ran before this adapter ────────────────────────

#[tokio::test]
async fn path_style_api_urls_are_recognised() {
    // What `S3StorageService` minted while it was the one pointed at R2. Not
    // recognising it would strand every reference it ever wrote.
    let svc = service().await;
    let url = format!("{API_ROOT}/{BUCKET}/avatars/a.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

#[tokio::test]
async fn virtual_hosted_api_urls_are_recognised() {
    let svc = service().await;
    let url = format!("https://{BUCKET}.abc123def456.r2.cloudflarestorage.com/avatars/a.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

#[tokio::test]
async fn the_canonical_endpoint_is_recognised_even_behind_an_override() {
    // Moving to a jurisdiction-restricted endpoint must not orphan the rows
    // written under the account's default one.
    let svc = R2StorageService::new(
        ACCOUNT,
        Some("https://abc123def456.eu.r2.cloudflarestorage.com"),
        BUCKET,
        "access",
        "secret",
        Some(PUBLIC_BASE),
        None,
    )
    .await
    .expect("valid config");

    for url in [
        format!("{API_ROOT}/{BUCKET}/avatars/a.png"),
        format!("https://abc123def456.eu.r2.cloudflarestorage.com/{BUCKET}/avatars/a.png"),
        format!("https://{BUCKET}.abc123def456.eu.r2.cloudflarestorage.com/avatars/a.png"),
    ] {
        assert_eq!(
            svc.key_from_url(&url).as_deref(),
            Some("avatars/a.png"),
            "lost `{url}`"
        );
    }
}

#[tokio::test]
async fn legacy_same_origin_urls_are_still_recognised() {
    // `/files/{key}` is what `ports::file_url` persists, so it is the shape that
    // actually dominates the database whichever backend is live.
    let svc = service().await;
    assert_eq!(
        svc.key_from_url("/files/avatars/a.png").as_deref(),
        Some("avatars/a.png")
    );
}

#[tokio::test]
async fn a_legacy_cdn_base_is_still_recognised_though_it_can_never_mint() {
    // CDN_BASE_URL cannot configure this backend's public origin, but anyone who
    // ran R2 through the S3 adapter had to set it. Both shapes under it resolve:
    // this adapter's object names, and the `/files/` paths written while the
    // same domain sat in front of database storage.
    let svc = build(Some(PUBLIC_BASE), Some("https://old-cdn.example.com"))
        .await
        .expect("valid config");

    assert_eq!(
        svc.key_from_url("https://old-cdn.example.com/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
    assert_eq!(
        svc.key_from_url("https://old-cdn.example.com/files/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
    // …and it is still not what gets minted.
    assert_eq!(svc.public_url("avatars/a.png"), format!("{PUBLIC_BASE}/avatars/a.png"));
}

#[tokio::test]
async fn a_public_base_mounted_under_files_resolves_to_the_bare_key() {
    // The natural choice for an operator moving off database storage who wants
    // existing URL shapes preserved. `under_base` must strip the base first, not
    // scan for `/files/` anywhere in the string.
    let svc = build(Some("https://cdn.example.com/files"), None)
        .await
        .expect("valid config");
    let url = svc.public_url("avatars/a.png");
    assert_eq!(url, "https://cdn.example.com/files/avatars/a.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

#[tokio::test]
async fn presigned_urls_and_cache_busters_resolve_to_the_same_key() {
    let svc = service().await;
    assert_eq!(
        svc.key_from_url(&format!(
            "{API_ROOT}/{BUCKET}/avatars/a.png?X-Amz-Signature=deadbeef&X-Amz-Expires=900"
        ))
        .as_deref(),
        Some("avatars/a.png")
    );
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/avatars/a.png?v=2")
            .as_deref(),
        Some("avatars/a.png")
    );
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/avatars/a.png#frag")
            .as_deref(),
        Some("avatars/a.png")
    );
}

// ─── What must NOT be claimed ────────────────────────────────────────────────

#[tokio::test]
async fn another_accounts_urls_are_not_claimed() {
    // R2 is multi-tenant on one hostname pattern, so the account id is the only
    // thing separating our objects from a stranger's. `key_from_url` feeds
    // `decrement_ref` and `delete_by_key`.
    let svc = service().await;
    for url in [
        format!("https://someone-else.r2.cloudflarestorage.com/{BUCKET}/avatars/a.png"),
        format!("https://{BUCKET}.someone-else.r2.cloudflarestorage.com/avatars/a.png"),
    ] {
        assert_eq!(svc.key_from_url(&url), None, "claimed `{url}`");
    }
}

#[tokio::test]
async fn another_buckets_urls_are_not_claimed() {
    let svc = service().await;
    for url in [
        format!("{API_ROOT}/other-bucket/avatars/a.png"),
        "https://other-bucket.abc123def456.r2.cloudflarestorage.com/avatars/a.png".to_string(),
    ] {
        assert_eq!(svc.key_from_url(&url), None, "claimed `{url}`");
    }
}

#[tokio::test]
async fn an_account_id_that_merely_starts_with_ours_is_not_claimed() {
    let svc = service().await;
    assert_eq!(
        svc.key_from_url(&format!(
            "https://abc123def456evil.r2.cloudflarestorage.com/{BUCKET}/avatars/a.png"
        )),
        None
    );
}

#[tokio::test]
async fn a_host_that_merely_starts_with_our_public_base_is_not_claimed() {
    // The classic prefix-match bug: without a `/` boundary after the base,
    // `cdn.example.com.evil.com` matches a base of `cdn.example.com`.
    let svc = service().await;
    assert_eq!(
        svc.key_from_url("https://cdn.example.com.evil.com/avatars/a.png"),
        None
    );
}

#[tokio::test]
async fn foreign_files_urls_are_not_claimed() {
    let svc = service().await;
    assert_eq!(
        svc.key_from_url("https://anyone-at-all.example.com/files/avatars/a.png"),
        None
    );
}

#[tokio::test]
async fn an_empty_key_is_not_a_key() {
    let svc = service().await;
    assert_eq!(svc.key_from_url("/files/"), None);
    assert_eq!(svc.key_from_url(PUBLIC_BASE), None);
    assert_eq!(svc.key_from_url(&format!("{PUBLIC_BASE}/")), None);
    assert_eq!(svc.key_from_url(&format!("{API_ROOT}/{BUCKET}/")), None);
}

// ─── Constructor rules ───────────────────────────────────────────────────────

#[tokio::test]
async fn a_missing_public_base_is_refused() {
    // The central guarantee: R2 without a public origin cannot start, because
    // there is no correct URL for it to mint and what it would write is
    // unrepairable.
    let message = construction_error(build(None, None).await, "no R2_PUBLIC_BASE_URL");
    assert!(
        message.contains("R2_PUBLIC_BASE_URL"),
        "the error must name the variable: {message}"
    );
}

#[tokio::test]
async fn cdn_base_url_does_not_stand_in_for_the_public_base() {
    // Deliberate: this value is written into content that is never rewritten, so
    // it is worth naming explicitly rather than inferring from a variable that
    // means something else for the other three backends.
    let message = construction_error(
        build(None, Some("https://cdn.example.com")).await,
        "CDN_BASE_URL is not a substitute",
    );
    assert!(message.contains("R2_PUBLIC_BASE_URL"), "unexpected: {message}");
}

#[tokio::test]
async fn a_public_base_pointing_at_the_api_endpoint_is_refused() {
    for base in [
        API_ROOT,
        "https://abc123def456.r2.cloudflarestorage.com/forum-uploads",
        "https://abc123def456.eu.r2.cloudflarestorage.com",
    ] {
        let message = construction_error(
            build(Some(base), None).await,
            "the API endpoint serves signed requests only",
        );
        assert!(
            message.contains("signed requests only"),
            "unexpected for `{base}`: {message}"
        );
    }
}

#[tokio::test]
async fn a_schemeless_public_base_is_refused() {
    // Without a scheme `public_url` yields a relative path, so every image
    // silently resolves against the page's own origin instead — and the CSP
    // widening and the public-read probe both read it as same-origin.
    let message = construction_error(
        build(Some("cdn.example.com"), None).await,
        "no scheme",
    );
    assert!(message.contains("absolute URL"), "unexpected: {message}");
}

#[tokio::test]
async fn a_missing_account_or_bucket_is_refused() {
    assert!(
        R2StorageService::new("  ", None, BUCKET, "a", "s", Some(PUBLIC_BASE), None)
            .await
            .is_err()
    );
    assert!(
        R2StorageService::new(ACCOUNT, None, "  ", "a", "s", Some(PUBLIC_BASE), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn missing_credentials_are_refused_at_construction() {
    // Failing the deploy beats failing the first avatar upload hours later with
    // an opaque signature error.
    let message = construction_error(
        R2StorageService::new(ACCOUNT, None, BUCKET, "", "", Some(PUBLIC_BASE), None).await,
        "no credentials",
    );
    assert!(
        message.contains("R2_ACCESS_KEY") && message.contains("R2_SECRET_KEY"),
        "unexpected: {message}"
    );
}
