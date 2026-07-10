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

// ─── usage_since (per-account upload quota) ───────────────────────────────────
//
// Exercises the raw aggregate SQL the compiler cannot check: `SUM(size)` yields
// `numeric` in Postgres, so the `::bigint` casts are what keep it deserializable
// into i64, and `COALESCE` is what keeps the zero-row case from returning NULL.

async fn make_user(db: &sea_orm::DatabaseConnection, n: u8) -> uuid::Uuid {
    use ferum_domain::repositories::user_repository::{NewUser, UserRepository};
    ferum_infrastructure::repositories::PgUserRepository::new(db.clone())
        .create(NewUser {
            username: format!("uploader{n}"),
            email: format!("uploader{n}@example.com"),
            password_hash: Some("$2b$12$fakehash".to_string()),
        })
        .await
        .expect("create test user")
        .id
}

#[tokio::test]
async fn usage_since_on_empty_schema_returns_zero_not_null() {
    let db = TestDb::new("sfile_usage_empty").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    // No rows at all: SUM(size) is NULL, so COALESCE(...)::bigint is what makes
    // this deserialize rather than error.
    let usage = repo
        .usage_since(uuid::Uuid::new_v4(), chrono::Utc::now() - chrono::Duration::hours(24))
        .await
        .expect("usage_since must handle the zero-row case");

    assert_eq!(usage.file_count, 0);
    assert_eq!(usage.total_bytes, 0);

    db.teardown().await;
}

#[tokio::test]
async fn usage_since_sums_only_this_users_files_inside_the_window() {
    let db = TestDb::new("sfile_usage_scoped").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    let alice = make_user(&db.conn, 1).await;
    let bob = make_user(&db.conn, 2).await;

    repo.upsert_and_ref("post-attachments/a1.png", "image/png", b"aaaa", 4, Some(alice))
        .await
        .expect("alice upload 1");
    repo.upsert_and_ref("post-attachments/a2.png", "image/png", b"bbbbbb", 6, Some(alice))
        .await
        .expect("alice upload 2");
    // Bob's upload must not be charged to Alice's quota.
    repo.upsert_and_ref("post-attachments/b1.png", "image/png", b"cccccccc", 8, Some(bob))
        .await
        .expect("bob upload");

    let since = chrono::Utc::now() - chrono::Duration::hours(24);
    let alice_usage = repo.usage_since(alice, since).await.expect("alice usage");
    assert_eq!(alice_usage.file_count, 2, "only Alice's two files");
    assert_eq!(alice_usage.total_bytes, 10, "4 + 6, excluding Bob's 8");

    let bob_usage = repo.usage_since(bob, since).await.expect("bob usage");
    assert_eq!(bob_usage.file_count, 1);
    assert_eq!(bob_usage.total_bytes, 8);

    // A window that starts in the future excludes everything — proves the
    // created_at >= $2 bound is actually applied, not ignored.
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    let windowed = repo.usage_since(alice, future).await.expect("future window");
    assert_eq!(windowed.file_count, 0, "rows older than the window are excluded");
    assert_eq!(windowed.total_bytes, 0);

    db.teardown().await;
}

// ─── upsert_staged / increment_ref (post-attachment publish gate) ─────────────

/// Reads ref_count directly; the trait exposes no getter and `decrement_ref`
/// would mutate the value under test.
async fn ref_count_of(db: &sea_orm::DatabaseConnection, key: &str) -> Option<i32> {
    use sea_orm::EntityTrait;
    ferum_infrastructure::entities::stored_files::Entity::find_by_id(key)
        .one(db)
        .await
        .expect("query stored_files")
        .map(|m| m.ref_count)
}

#[tokio::test]
async fn upsert_staged_inserts_with_ref_count_zero() {
    let db = TestDb::new("sfile_staged_insert").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_staged("post-attachments/deadbeef.png", "image/png", b"img", 3, None)
        .await
        .expect("upsert_staged inserts");

    assert_eq!(
        ref_count_of(&db.conn, "post-attachments/deadbeef.png").await,
        Some(0),
        "a staged attachment must start unreferenced, so /files/ will not publish it"
    );

    db.teardown().await;
}

#[tokio::test]
async fn upsert_staged_is_idempotent_and_never_resets_an_existing_ref_count() {
    let db = TestDb::new("sfile_staged_conflict").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "post-attachments/shared.png";

    // A live post already references these exact bytes.
    repo.upsert_staged(key, "image/png", b"img", 3, None).await.expect("stage");
    repo.increment_ref(key).await.expect("a post embeds it");
    assert_eq!(ref_count_of(&db.conn, key).await, Some(1));

    // Someone re-uploads identical bytes (CAS collapses them to one row).
    // ON CONFLICT DO NOTHING: must neither reset to 0 nor bump to 2.
    repo.upsert_staged(key, "image/png", b"img", 3, None)
        .await
        .expect("re-staging an existing key must not error");

    assert_eq!(
        ref_count_of(&db.conn, key).await,
        Some(1),
        "re-staging must not unpublish, nor inflate, an already-referenced file"
    );

    db.teardown().await;
}

#[tokio::test]
async fn increment_ref_on_missing_key_is_a_noop_not_an_error() {
    let db = TestDb::new("sfile_incr_missing").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    // Post content can embed any /files/... URL, including one never uploaded.
    repo.increment_ref("post-attachments/never-uploaded.png")
        .await
        .expect("increment_ref on an unknown key must not fail post creation");

    assert_eq!(ref_count_of(&db.conn, "post-attachments/never-uploaded.png").await, None);

    db.teardown().await;
}

#[tokio::test]
async fn publish_then_unpublish_round_trip() {
    let db = TestDb::new("sfile_publish_cycle").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "post-attachments/cycle.png";

    repo.upsert_staged(key, "image/png", b"img", 3, None).await.expect("stage");

    // Two posts embed the same image.
    repo.increment_ref(key).await.expect("post A");
    repo.increment_ref(key).await.expect("post B");
    assert_eq!(ref_count_of(&db.conn, key).await, Some(2));

    // Deleting post A must leave it published for post B.
    assert_eq!(repo.decrement_ref(key).await.expect("delete A"), 1);

    // Deleting post B unpublishes it — but the row survives (no GC here).
    assert_eq!(repo.decrement_ref(key).await.expect("delete B"), 0);
    assert_eq!(
        ref_count_of(&db.conn, key).await,
        Some(0),
        "unpublishing must not delete the blob"
    );

    db.teardown().await;
}
