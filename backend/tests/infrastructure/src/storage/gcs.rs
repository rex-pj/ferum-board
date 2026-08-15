//! Offline unit tests for [`GcsStorageService`]: URL round-tripping, object-name
//! prefixing and credential parsing.
//!
//! **`put`/`delete` are compile-checked only** — change their request shape and
//! nothing here will tell you. Covered instead is the half that fails silently:
//! an unrecognised URL leaks or collects a CAS reference without erroring, so
//! every accepted AND refused shape gets an assertion.
//!
//! Credentials are a fake `authorized_user` document — it parses without an RSA
//! key, so construction touches neither the ADC path nor the network.

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;
use ferum_infrastructure::storage::GcsStorageService;

const BUCKET: &str = "forum-uploads";

const FAKE_CREDENTIALS: &str = r#"{
    "type": "authorized_user",
    "client_id": "test.apps.googleusercontent.com",
    "client_secret": "not-a-real-secret",
    "refresh_token": "1//not-a-real-token"
}"#;

fn service(prefix: Option<&str>, cdn: Option<&str>) -> GcsStorageService {
    GcsStorageService::new(BUCKET, prefix, cdn, Some(FAKE_CREDENTIALS), None)
        .expect("constructing with valid authorized-user credentials")
}

/// `Result::expect_err` needs `T: Debug`, and the service deliberately does not
/// derive it — one of its fields is a private signing key.
fn construction_error(result: Result<GcsStorageService, AppError>, why: &str) -> String {
    match result {
        Ok(_) => panic!("expected construction to fail: {why}"),
        Err(e) => e.to_string(),
    }
}

// ─── public_url / key_from_url round-tripping ─────────────────────────────────

#[test]
fn path_style_url_round_trips() {
    let svc = service(None, None);
    let url = svc.public_url("avatars/abc123.png");
    assert_eq!(
        url,
        "https://storage.googleapis.com/forum-uploads/avatars/abc123.png"
    );
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/abc123.png"));
}

#[test]
fn cdn_prefixed_url_round_trips() {
    let svc = service(None, Some("https://cdn.example.com"));
    let url = svc.public_url("avatars/abc123.png");
    assert_eq!(url, "https://cdn.example.com/avatars/abc123.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/abc123.png"));
}

#[test]
fn a_trailing_slash_on_the_cdn_base_does_not_double_up() {
    let svc = service(None, Some("https://cdn.example.com/"));
    assert_eq!(svc.public_url("k.png"), "https://cdn.example.com/k.png");
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/k.png").as_deref(),
        Some("k.png")
    );
}

// ─── The legacy same-origin form ──────────────────────────────────────────────
//
// The migration case that must not break: an install that ran on database
// storage has `/files/{key}` baked into user rows, site config and the stored
// HTML of every post. Those strings are never rewritten, and the rows keep their
// bytes, so they have to stay resolvable after the switch to GCS.

#[test]
fn legacy_same_origin_urls_are_still_recognised() {
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("/files/avatars/old.png").as_deref(),
        Some("avatars/old.png")
    );
}

#[test]
fn legacy_cdn_prefixed_urls_are_still_recognised() {
    // Written by `DatabaseStorageService::public_url` while CDN_BASE_URL was set.
    let svc = service(None, Some("https://cdn.example.com"));
    assert_eq!(
        svc.key_from_url("https://cdn.example.com/files/avatars/old.png")
            .as_deref(),
        Some("avatars/old.png"),
        "the legacy /files/ segment must win over the bare {{cdn}}/{{key}} shape"
    );
}

#[test]
fn legacy_urls_are_not_subjected_to_the_gcs_prefix() {
    // A `/files/` URL predates this backend, so it cannot carry GCS_PREFIX.
    // Stripping the prefix from it would fail to find one and return None —
    // silently orphaning every file uploaded before the switch.
    let svc = service(Some("ferum"), None);
    assert_eq!(
        svc.key_from_url("/files/avatars/old.png").as_deref(),
        Some("avatars/old.png")
    );
}

// ─── Every other shape a GCS object can be served under ───────────────────────

#[test]
fn virtual_hosted_urls_are_recognised() {
    // What most SDKs and `gsutil`-adjacent tooling emit.
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://forum-uploads.storage.googleapis.com/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
}

#[test]
fn console_authenticated_download_urls_are_recognised() {
    // `storage.cloud.google.com` is what the Cloud Console's "copy link" button
    // produces, so it genuinely ends up pasted into posts.
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://storage.cloud.google.com/forum-uploads/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
}

#[test]
fn mtls_endpoint_urls_are_recognised() {
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://storage.mtls.googleapis.com/forum-uploads/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
    assert_eq!(
        svc.key_from_url("https://forum-uploads.storage.mtls.googleapis.com/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
}

#[test]
fn custom_domain_cname_target_urls_are_recognised() {
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://c.storage.googleapis.com/forum-uploads/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
}

#[test]
fn plain_http_variants_are_recognised() {
    // The CNAME custom-domain form is documented over plain HTTP.
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("http://storage.googleapis.com/forum-uploads/avatars/a.png")
            .as_deref(),
        Some("avatars/a.png")
    );
}

#[test]
fn signed_urls_resolve_to_the_same_key() {
    // A V4 signed URL is the public URL plus an authorization query string. The
    // query is no part of the key, and leaving it attached turns a URL we minted
    // into one we no longer recognise.
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url(
            "https://storage.googleapis.com/forum-uploads/avatars/a.png\
             ?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Expires=900&X-Goog-Signature=deadbeef"
        )
        .as_deref(),
        Some("avatars/a.png")
    );
}

#[test]
fn a_fragment_is_not_part_of_the_key() {
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://storage.googleapis.com/forum-uploads/avatars/a.png#top")
            .as_deref(),
        Some("avatars/a.png")
    );
}

// ─── URLs that must NOT be claimed ────────────────────────────────────────────

#[test]
fn another_buckets_urls_are_not_claimed() {
    // Claiming one would decrement a reference count belonging to data this
    // deployment does not own.
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://storage.googleapis.com/someone-elses-bucket/avatars/a.png"),
        None
    );
    assert_eq!(
        svc.key_from_url("https://someone-elses-bucket.storage.googleapis.com/avatars/a.png"),
        None
    );
}

#[test]
fn a_bucket_name_that_merely_starts_with_ours_is_not_claimed() {
    let svc = service(None, None);
    assert_eq!(
        svc.key_from_url("https://storage.googleapis.com/forum-uploads-staging/avatars/a.png"),
        None
    );
}

#[test]
fn foreign_urls_are_not_claimed() {
    // An author may paste any external image into a post.
    let svc = service(None, None);
    assert_eq!(svc.key_from_url("https://example.com/photo.png"), None);
}

#[test]
fn an_empty_key_is_not_a_key() {
    let svc = service(None, None);
    assert_eq!(svc.key_from_url("/files/"), None);
    assert_eq!(
        svc.key_from_url("https://storage.googleapis.com/forum-uploads/"),
        None
    );
}

#[test]
fn an_unconfigured_cdn_does_not_match_everything() {
    // `strip_base` refuses an empty base for exactly this reason.
    let svc = service(None, None);
    assert_eq!(svc.key_from_url("/some/relative/path.png"), None);
}

// ─── GCS_PREFIX ───────────────────────────────────────────────────────────────

#[test]
fn a_prefix_is_applied_on_write_and_stripped_on_read() {
    let svc = service(Some("ferum"), None);
    let url = svc.public_url("avatars/a.png");
    assert_eq!(
        url,
        "https://storage.googleapis.com/forum-uploads/ferum/avatars/a.png"
    );
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

#[test]
fn a_prefix_round_trips_through_the_cdn_shape_too() {
    let svc = service(Some("ferum"), Some("https://cdn.example.com"));
    let url = svc.public_url("avatars/a.png");
    assert_eq!(url, "https://cdn.example.com/ferum/avatars/a.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/a.png"));
}

#[test]
fn objects_outside_the_prefix_belong_to_someone_else() {
    // The whole point of GCS_PREFIX is a bucket shared with other workloads.
    let svc = service(Some("ferum"), None);
    assert_eq!(
        svc.key_from_url("https://storage.googleapis.com/forum-uploads/other-app/a.png"),
        None
    );
}

#[test]
fn prefix_slashes_are_normalised() {
    // `/ferum/`, `ferum/` and `ferum` must all mean the same thing.
    for written in ["ferum", "/ferum", "ferum/", "/ferum/"] {
        let svc = service(Some(written), None);
        assert_eq!(
            svc.public_url("a.png"),
            "https://storage.googleapis.com/forum-uploads/ferum/a.png",
            "GCS_PREFIX written as `{written}`"
        );
    }
}

// ─── Credential handling ──────────────────────────────────────────────────────

#[test]
fn a_missing_bucket_is_rejected() {
    assert!(GcsStorageService::new("   ", None, None, Some(FAKE_CREDENTIALS), None).is_err());
}

#[test]
fn malformed_credentials_fail_at_construction_not_at_first_upload() {
    // Failing the deploy beats failing the first avatar upload hours later.
    let message = construction_error(
        GcsStorageService::new(BUCKET, None, None, Some("{not json"), None),
        "malformed credential JSON",
    );
    assert!(message.contains("not valid JSON"), "unexpected: {message}");
}

#[test]
fn a_service_account_document_missing_its_key_is_rejected() {
    let message = construction_error(
        GcsStorageService::new(
            BUCKET,
            None,
            None,
            Some(r#"{"type":"service_account","client_email":"a@b.iam.gserviceaccount.com"}"#),
            None,
        ),
        "a service account with no private_key cannot sign anything",
    );
    assert!(message.contains("private_key"), "unexpected: {message}");
}

#[test]
fn an_unsupported_credential_type_says_so_instead_of_silently_falling_back() {
    // `external_account` is Workload Identity Federation. Treating it as "no
    // credentials" would silently switch to the metadata server and surface as
    // a 401 that looks like a bucket permission problem.
    let message = construction_error(
        GcsStorageService::new(BUCKET, None, None, Some(r#"{"type":"external_account"}"#), None),
        "external_account is not supported",
    );
    assert!(message.contains("external_account"), "unexpected: {message}");
    assert!(message.contains("metadata server"), "unexpected: {message}");
}

#[test]
fn an_unreadable_credentials_file_is_reported_with_its_path() {
    let message = construction_error(
        GcsStorageService::new(BUCKET, None, None, None, Some("/no/such/gcs-key.json")),
        "a configured-but-missing credentials file is an operator error",
    );
    assert!(
        message.contains("/no/such/gcs-key.json"),
        "the message must name the path so it can be fixed: {message}"
    );
}
