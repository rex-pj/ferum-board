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
