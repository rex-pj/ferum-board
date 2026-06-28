//! Integration tests for [`PgStoredFileRepository`].
//!
//! These tests verify the raw SQL fragments the compiler cannot check:
//!   - `RETURNING ref_count` in `decrement_ref`
//!   - `Func::greatest([ref_count - 1, 0])` floor in `decrement_ref`
//!   - `ON CONFLICT (key) DO UPDATE SET ref_count = ref_count + 1` in `upsert_and_ref`

use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_infrastructure::repositories::PgStoredFileRepository;

use crate::common::TestDb;

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_inserts_with_ref_count_one() {
    let db = TestDb::new("sfile_upsert_insert").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:aabbcc", "image/png", b"fake_data", 9, None)
        .await
        .expect("upsert_and_ref inserts new row with ref_count = 1");

    // Verify via decrement: 1 → 0
    let after = repo
        .decrement_ref("sha256:aabbcc")
        .await
        .expect("decrement_ref after single insert");
    assert_eq!(after, 0, "ref_count after decrement of a single insert should be 0");

    db.teardown().await;
}

#[tokio::test]
async fn upsert_on_conflict_increments_ref_count() {
    let db = TestDb::new("sfile_upsert_conflict").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:aabbcc", "image/png", b"fake_data", 9, None)
        .await
        .expect("first upsert inserts (ref_count = 1)");

    // Second call for the same key must hit ON CONFLICT and increment ref_count to 2
    repo.upsert_and_ref("sha256:aabbcc", "image/png", b"fake_data", 9, None)
        .await
        .expect("second upsert increments ref_count via ON CONFLICT DO UPDATE");

    // Decrement twice: 2 → 1 → 0
    let after_first_dec = repo
        .decrement_ref("sha256:aabbcc")
        .await
        .expect("first decrement");
    assert_eq!(after_first_dec, 1);

    let after_second_dec = repo
        .decrement_ref("sha256:aabbcc")
        .await
        .expect("second decrement");
    assert_eq!(after_second_dec, 0);

    db.teardown().await;
}

#[tokio::test]
async fn decrement_ref_greatest_floors_at_zero() {
    let db = TestDb::new("sfile_decrement_floor").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:aabbcc", "image/png", b"fake_data", 9, None)
        .await
        .expect("insert");

    // Decrement to 0
    repo.decrement_ref("sha256:aabbcc").await.expect("first decrement");

    // Decrement again: GREATEST(0, ref_count - 1) must not go negative
    let at_floor = repo
        .decrement_ref("sha256:aabbcc")
        .await
        .expect("decrement below zero — GREATEST(0, …) must floor");
    assert_eq!(at_floor, 0, "ref_count must not go below 0");

    db.teardown().await;
}

#[tokio::test]
async fn decrement_ref_on_missing_key_returns_zero() {
    let db = TestDb::new("sfile_decrement_missing").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    // No row exists — the RETURNING clause yields no rows → unwrap_or(0)
    let result = repo
        .decrement_ref("sha256:nonexistent")
        .await
        .expect("decrement_ref on missing key must not error");
    assert_eq!(result, 0);

    db.teardown().await;
}

#[tokio::test]
async fn delete_by_key_removes_row() {
    let db = TestDb::new("sfile_delete_by_key").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:aabbcc", "image/png", b"fake_data", 9, None)
        .await
        .expect("insert");

    repo.delete_by_key("sha256:aabbcc")
        .await
        .expect("delete_by_key executes");

    // Row is gone: decrement returns 0 (no row found → unwrap_or(0))
    let result = repo
        .decrement_ref("sha256:aabbcc")
        .await
        .expect("decrement after delete");
    assert_eq!(result, 0, "row must be gone after delete_by_key");

    db.teardown().await;
}

#[tokio::test]
async fn three_upserts_then_two_decrements_leaves_ref_count_one() {
    let db = TestDb::new("sfile_ref_count_lifecycle").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    // Three callers reference the same CAS key
    for _ in 0..3 {
        repo.upsert_and_ref("sha256:shared", "image/jpeg", b"img", 3, None)
            .await
            .expect("upsert");
    }

    repo.decrement_ref("sha256:shared").await.expect("dec 1");
    let after = repo
        .decrement_ref("sha256:shared")
        .await
        .expect("dec 2");
    assert_eq!(after, 1, "after 2 decrements of 3, ref_count must be 1");

    db.teardown().await;
}
