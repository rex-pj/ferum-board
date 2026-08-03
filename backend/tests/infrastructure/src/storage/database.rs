//! Integration tests for [`DatabaseStorageService`].
//!
//! The service stores raw blobs in a `stored_files` table row and serves them at `/files/:key`.
//! Critical: ON CONFLICT upsert path must update content_type + data + size.

use bytes::Bytes;
use ferum_application::ports::StorageService;
use ferum_infrastructure::storage::DatabaseStorageService;

use crate::common::TestDb;

#[tokio::test]
async fn put_and_public_url() {
    let db = TestDb::new("dbs_put_url").await;
    let svc = DatabaseStorageService::new(db.conn.clone());

    svc.put("sha256-abc", Bytes::from(b"hello".as_ref()), "text/plain")
        .await.expect("put");
    assert_eq!(svc.public_url("sha256-abc"), "/files/sha256-abc");
    db.teardown().await;
}

#[tokio::test]
async fn put_upserts_existing_key() {
    // ON CONFLICT must update the row, not fail with a duplicate-key error.
    let db = TestDb::new("dbs_put_upsert").await;
    let svc = DatabaseStorageService::new(db.conn.clone());

    svc.put("sha256-key1", Bytes::from(b"v1".as_ref()), "text/plain").await.expect("first put");
    svc.put("sha256-key1", Bytes::from(b"v2".as_ref()), "image/png").await.expect("second put (upsert)");
    db.teardown().await;
}

#[tokio::test]
async fn delete_existing_key_succeeds() {
    let db = TestDb::new("dbs_delete").await;
    let svc = DatabaseStorageService::new(db.conn.clone());

    svc.put("sha256-to-delete", Bytes::from(b"data".as_ref()), "application/octet-stream")
        .await.expect("put");
    svc.delete("sha256-to-delete").await.expect("delete");
    db.teardown().await;
}

#[tokio::test]
async fn delete_nonexistent_key_does_not_error() {
    let db = TestDb::new("dbs_delete_missing").await;
    let svc = DatabaseStorageService::new(db.conn.clone());
    // Should silently succeed (no row found is not an error)
    assert!(svc.delete("sha256-ghost").await.is_ok());
    db.teardown().await;
}

#[tokio::test]
async fn public_url_returns_files_path_prefix() {
    let db = TestDb::new("dbs_public_url").await;
    let svc = DatabaseStorageService::new(db.conn.clone());
    let url = svc.public_url("sha256-abc123def456");
    assert!(url.starts_with("/files/"), "public_url must start with /files/");
    assert!(url.contains("sha256-abc123def456"));
    db.teardown().await;
}

// ─── URL round-tripping ───────────────────────────────────────────────────────
//
// `public_url` and `key_from_url` are inverses, and the pair carries more weight
// than its size suggests: file URLs are denormalised into user rows, site config
// and the stored HTML of every post, and the *reverse* direction is what drives
// reference counting. A `key_from_url` that fails to recognise a URL does not
// error — it silently reports "not one of ours", so the reference is never taken
// or never released, and files are either leaked or collected out from under
// posts still displaying them.

#[tokio::test]
async fn same_origin_url_round_trips() {
    let db = TestDb::new("dbs_url_same_origin").await;
    let svc = DatabaseStorageService::new(db.conn.clone());
    let url = svc.public_url("avatars/abc123.png");
    assert_eq!(url, "/files/avatars/abc123.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/abc123.png"));
    db.teardown().await;
}

#[tokio::test]
async fn cdn_prefixed_url_round_trips() {
    let db = TestDb::new("dbs_url_cdn").await;
    let svc = DatabaseStorageService::new(db.conn.clone())
        .with_cdn_base_url(Some("https://cdn.example.com"));
    let url = svc.public_url("avatars/abc123.png");
    assert_eq!(url, "https://cdn.example.com/files/avatars/abc123.png");
    assert_eq!(svc.key_from_url(&url).as_deref(), Some("avatars/abc123.png"));
    db.teardown().await;
}

/// The compatibility case that matters: adding `CDN_BASE_URL` to a running forum
/// must not orphan the URLs already written into posts and profiles.
#[tokio::test]
async fn a_cdn_configured_backend_still_reads_legacy_same_origin_urls() {
    let db = TestDb::new("dbs_url_legacy").await;
    let svc = DatabaseStorageService::new(db.conn.clone())
        .with_cdn_base_url(Some("https://cdn.example.com"));
    assert_eq!(
        svc.key_from_url("/files/avatars/old.png").as_deref(),
        Some("avatars/old.png"),
        "URLs written before the CDN was configured must stay resolvable"
    );
    db.teardown().await;
}

#[tokio::test]
async fn a_trailing_slash_on_the_cdn_base_does_not_double_up() {
    let db = TestDb::new("dbs_url_slash").await;
    let svc = DatabaseStorageService::new(db.conn.clone())
        .with_cdn_base_url(Some("https://cdn.example.com/"));
    assert_eq!(svc.public_url("k.png"), "https://cdn.example.com/files/k.png");
    db.teardown().await;
}

#[tokio::test]
async fn foreign_urls_are_not_claimed() {
    let db = TestDb::new("dbs_url_foreign").await;
    let svc = DatabaseStorageService::new(db.conn.clone());
    // An author may paste any external image into a post; claiming one as ours
    // would decrement a reference count that belongs to nothing.
    assert_eq!(svc.key_from_url("https://example.com/photo.png"), None);
    assert_eq!(svc.key_from_url("/files/"), None, "an empty key is not a key");
    db.teardown().await;
}

/// `put` must leave `ref_count` alone — the repository call that follows takes
/// the first reference. The column DEFAULTs to 1, so a `put` that let that
/// default through would leave every new file already referenced once and
/// therefore never collectable.
#[tokio::test]
async fn put_does_not_take_a_reference() {
    use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
    let db = TestDb::new("dbs_put_refcount").await;
    let svc = DatabaseStorageService::new(db.conn.clone());
    let repo = ferum_infrastructure::repositories::PgStoredFileRepository::new(db.conn.clone());

    svc.put("sha256-fresh", Bytes::from(b"x".as_ref()), "image/png").await.expect("put");
    repo.upsert_and_ref("sha256-fresh", "image/png", 1, None).await.expect("ref");

    assert_eq!(
        repo.decrement_ref("sha256-fresh").await.expect("dec"),
        0,
        "one upload must leave exactly one reference"
    );
    db.teardown().await;
}
