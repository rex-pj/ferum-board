//! Integration tests for [`PgStoredFileRepository`].
//!
//! These tests verify the raw SQL fragments the compiler cannot check:
//!   - `RETURNING ref_count` in `decrement_ref`
//!   - `Func::greatest([ref_count - 1, 0])` floor in `decrement_ref`
//!   - `ON CONFLICT (key) DO UPDATE SET ref_count = ref_count + 1` in `upsert_and_ref`
//!   - the six-way UNION in `referencing_pointers`, which names five tables and
//!     six columns in a string literal — every one of them a runtime failure if
//!     misspelled, and one that only fires when an admin runs the audit

use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_infrastructure::repositories::PgStoredFileRepository;
use sea_orm::{ConnectionTrait, Statement};

use crate::common::TestDb;

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_inserts_with_ref_count_one() {
    let db = TestDb::new("sfile_upsert_insert").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:aabbcc", "image/png", 9, None)
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

    repo.upsert_and_ref("sha256:aabbcc", "image/png", 9, None)
        .await
        .expect("first upsert inserts (ref_count = 1)");

    // Second call for the same key must hit ON CONFLICT and increment ref_count to 2
    repo.upsert_and_ref("sha256:aabbcc", "image/png", 9, None)
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

    repo.upsert_and_ref("sha256:aabbcc", "image/png", 9, None)
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

/// Inserts a minimal user and returns its id.
///
/// Needed because `stored_files.uploaded_by_id` carries a foreign key to
/// `users` — which is itself worth knowing: the column can never hold an id that
/// is not a real account, so the production path passing `Some(actor.id)` is
/// always valid by construction.
async fn a_user(db: &crate::common::TestDb, name: &str) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    db.conn
        .execute_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            format!(
                "INSERT INTO users (id, username, email)
                 VALUES ('{id}', '{name}', '{name}@example.test')"
            ),
        ))
        .await
        .expect("seed a user");
    id
}

/// Reads `uploaded_by_id` back, since no repository method exposes it.
async fn owner_of(db: &crate::common::TestDb, key: &str) -> Option<uuid::Uuid> {
    db.conn
        .query_one_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            format!("SELECT uploaded_by_id FROM stored_files WHERE key = '{key}'"),
        ))
        .await
        .expect("query")
        .expect("row")
        .try_get::<Option<uuid::Uuid>>("", "uploaded_by_id")
        .expect("column")
}

#[tokio::test]
async fn the_uploader_is_recorded_even_when_the_row_already_exists() {
    // Under database storage `put` inserts the row before the repository is
    // asked to reference it, so the upsert always takes its conflict path. While
    // that path did not name `uploaded_by_id`, the column was NULL for every file
    // in the system — which silently disabled the per-account upload quota
    // (`usage_since` filters on it) and made every staged attachment 404 to its
    // own uploader (`resolve_stored_file` requires a non-NULL owner). Neither
    // was visible under an object store, where the row does not pre-exist.
    let db = TestDb::new("sfile_owner_on_conflict").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "avatars/deadbeef";
    let alice = a_user(&db, "alice_owner").await;

    // Stands in for `DatabaseStorageService::put`: the row exists first, with no
    // owner and ref_count 0.
    db.conn
        .execute_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            format!(
                "INSERT INTO stored_files (key, content_type, size, ref_count)
                 VALUES ('{key}', 'image/jpeg', 10, 0)"
            ),
        ))
        .await
        .expect("seed the pre-existing row");
    assert_eq!(owner_of(&db, key).await, None, "precondition");

    repo.upsert_and_ref(key, "image/jpeg", 10, Some(alice))
        .await
        .expect("reference it");
    assert_eq!(
        owner_of(&db, key).await,
        Some(alice),
        "the conflict path must record the uploader"
    );

    db.teardown().await;
}

#[tokio::test]
async fn a_deduplicated_upload_does_not_reassign_the_owner() {
    // CAS keys are content digests, so two users uploading the same bytes land on
    // one row. Overwriting the owner would move somebody else's file — and their
    // quota — onto the second uploader.
    let db = TestDb::new("sfile_owner_no_steal").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "avatars/shared";
    let alice = a_user(&db, "alice_dedupe").await;
    let bob = a_user(&db, "bob_dedupe").await;

    repo.upsert_and_ref(key, "image/jpeg", 10, Some(alice))
        .await
        .expect("first upload");
    repo.upsert_and_ref(key, "image/jpeg", 10, Some(bob))
        .await
        .expect("identical bytes from someone else");

    assert_eq!(
        owner_of(&db, key).await,
        Some(alice),
        "first named uploader wins"
    );

    db.teardown().await;
}

#[tokio::test]
async fn staging_records_the_uploader_and_leaves_ref_count_alone() {
    // `upsert_staged` used to be DO NOTHING, which skipped the owner too. The
    // replacement must still not touch `ref_count`: an existing key may already
    // be referenced by live posts.
    let db = TestDb::new("sfile_staged_owner").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "post-attachments/abcd";
    let alice = a_user(&db, "alice_staged").await;

    db.conn
        .execute_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            format!(
                "INSERT INTO stored_files (key, content_type, size, ref_count)
                 VALUES ('{key}', 'image/jpeg', 10, 3)"
            ),
        ))
        .await
        .expect("seed a referenced row");

    repo.upsert_staged(key, "image/jpeg", 10, Some(alice))
        .await
        .expect("stage it");

    assert_eq!(owner_of(&db, key).await, Some(alice));
    // Read the count back through the only method that reports it.
    assert_eq!(
        repo.decrement_ref(key).await.expect("decrement"),
        2,
        "staging must neither reset nor bump an existing ref_count"
    );

    db.teardown().await;
}

#[tokio::test]
async fn referencing_pointers_names_columns_that_actually_exist() {
    // The point of this test is that the query *runs*. Its SQL is a string
    // literal naming `user_avatars.file_key`, `user_covers.file_key`,
    // `thread_thumbnails.file_key`, `product_media.storage_key`,
    // `themes.preview_url` and `site_config.value` — six chances for a rename to
    // turn the audit into a 500 nobody sees until they use it.
    //
    // This is not a hypothetical. The first draft of that query was written
    // against the schema block in CLAUDE.md, which claimed `users.avatar_url`
    // and `threads.thumbnail_url` were columns. They are not — both are side
    // tables — and this test is what said so, on its first run.
    //
    // An empty result on a fresh database is the strongest cheap assertion
    // available: reaching it means Postgres resolved every table and column.
    let db = TestDb::new("sfile_referencing_pointers").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    let empty = repo
        .referencing_pointers()
        .await
        .expect("the UNION must resolve against the real schema");
    assert!(
        empty.is_empty(),
        "a database with no uploads references nothing"
    );

    // And one real pointer, through the branch with a WHERE clause — proof the
    // filter does not exclude everything. `site_config` is the only referencing
    // table with no foreign keys, so it needs no fixture scaffolding.
    // `execute_raw`, not `execute`: in Sea-ORM 2.0 the un-suffixed form takes a
    // sea-query statement by reference, so a raw `Statement` does not fit it.
    db.conn
        .execute_raw(Statement::from_string(
            db.conn.get_database_backend(),
            "INSERT INTO site_config (key, value) VALUES ('logo_url', '/files/logos/x.png')
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value"
                .to_string(),
        ))
        .await
        .expect("seed a logo pointer");

    let found = repo.referencing_pointers().await.expect("query runs");
    assert_eq!(found, ["/files/logos/x.png"]);

    db.teardown().await;
}

#[tokio::test]
async fn a_still_referenced_row_survives_collection() {
    // Replaces the old `delete_by_key_removes_row`. That method is gone: it
    // deleted the row unconditionally and left the object behind, which is what
    // stranded logo, favicon and theme-preview files in the bucket. The
    // surviving path is `delete_if_unreferenced`, and the property worth pinning
    // is the one the old method could not offer — a row that got re-referenced
    // between the decrement and the sweep is left completely alone.
    let db = TestDb::new("sfile_delete_if_referenced").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:aabbcc", "image/png", 9, None)
        .await
        .expect("insert");
    repo.upsert_and_ref("sha256:aabbcc", "image/png", 9, None)
        .await
        .expect("second reference — CAS dedupe on identical content");
    repo.decrement_ref("sha256:aabbcc").await.expect("release one");

    assert!(
        !repo
            .delete_if_unreferenced("sha256:aabbcc")
            .await
            .expect("query runs"),
        "one reference remains, so nothing may be deleted"
    );

    let remaining = repo.decrement_ref("sha256:aabbcc").await.expect("release last");
    assert_eq!(remaining, 0);
    assert!(
        repo.delete_if_unreferenced("sha256:aabbcc")
            .await
            .expect("query runs"),
        "the last reference is gone, so the row must go"
    );

    db.teardown().await;
}

#[tokio::test]
async fn delete_if_unreferenced_removes_a_row_at_zero() {
    let db = TestDb::new("sfile_del_unref_zero").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:orphan", "image/png", 5, None)
        .await
        .expect("insert");
    repo.decrement_ref("sha256:orphan").await.expect("dec to 0");

    let deleted = repo
        .delete_if_unreferenced("sha256:orphan")
        .await
        .expect("conditional delete executes");

    assert!(deleted, "a row sitting at ref_count 0 must be collectable");
    db.teardown().await;
}

/// The GC race, reproduced against a real database.
///
/// Sequence: last reference released (count → 0, GC enqueued) → the identical
/// bytes are uploaded again before the job runs, which CAS folds back onto the
/// same row at count 1 → GC finally runs. The conditional DELETE must match
/// nothing, because that row is now somebody's live avatar.
///
/// This asserts the property the old `decrement_ref`-based check could not have:
/// it consumed the new reference and reported 0, deleting a file in active use.
#[tokio::test]
async fn delete_if_unreferenced_spares_a_row_revived_before_gc_ran() {
    let db = TestDb::new("sfile_del_unref_revived").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("sha256:revived", "image/png", 5, None)
        .await
        .expect("first upload");
    repo.decrement_ref("sha256:revived").await.expect("dec to 0");

    // The re-upload that races the queued GC job.
    repo.upsert_and_ref("sha256:revived", "image/png", 5, None)
        .await
        .expect("re-upload of identical bytes");

    let deleted = repo
        .delete_if_unreferenced("sha256:revived")
        .await
        .expect("conditional delete executes");

    assert!(!deleted, "a re-referenced row must survive GC");

    // And it survived intact — still referenced exactly once.
    let count = repo
        .decrement_ref("sha256:revived")
        .await
        .expect("row still exists");
    assert_eq!(count, 0, "the surviving row had exactly one live reference");

    db.teardown().await;
}

#[tokio::test]
async fn three_upserts_then_two_decrements_leaves_ref_count_one() {
    let db = TestDb::new("sfile_ref_count_lifecycle").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    // Three callers reference the same CAS key
    for _ in 0..3 {
        repo.upsert_and_ref("sha256:shared", "image/jpeg", 3, None)
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

    repo.upsert_and_ref("post-attachments/a1.png", "image/png", 4, Some(alice))
        .await
        .expect("alice upload 1");
    repo.upsert_and_ref("post-attachments/a2.png", "image/png", 6, Some(alice))
        .await
        .expect("alice upload 2");
    // Bob's upload must not be charged to Alice's quota.
    repo.upsert_and_ref("post-attachments/b1.png", "image/png", 8, Some(bob))
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

    repo.upsert_staged("post-attachments/deadbeef.png", "image/png", 3, None)
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
    repo.upsert_staged(key, "image/png", 3, None).await.expect("stage");
    repo.increment_ref(key).await.expect("a post embeds it");
    assert_eq!(ref_count_of(&db.conn, key).await, Some(1));

    // Someone re-uploads identical bytes (CAS collapses them to one row).
    // ON CONFLICT DO NOTHING: must neither reset to 0 nor bump to 2.
    repo.upsert_staged(key, "image/png", 3, None)
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

    repo.upsert_staged(key, "image/png", 3, None).await.expect("stage");

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
