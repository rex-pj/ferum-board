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
use ferum_infrastructure::repositories::stored_file_repository::ATTACHMENT_KEY_PREFIX;
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

#[tokio::test]
async fn the_age_floor_excludes_a_recently_staged_row() {
    // `attachment_keys_before` is the ONLY thing standing between the audit's
    // release pass and an image inside a composer somebody still has open: a
    // staged attachment is referenced by nothing, and will stay that way until
    // its author hits post. The floor is a SQL predicate on `created_at`, so
    // nothing in the application suite can vouch for it — the double there
    // ignores the cutoff it is handed.
    //
    // Both rows are attachments at `ref_count = 0`, differing only in age.
    let db = TestDb::new("sfile_age_floor").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    let fresh = "post-attachments/still-typing.png";
    let stale = "post-attachments/abandoned.png";
    for key in [fresh, stale] {
        repo.upsert_staged(key, "image/png", 4, None).await.expect("stage");
    }
    // `created_at` defaults to now(), so the old one is aged by hand — this is
    // the one thing a test cannot do by waiting.
    // `execute_raw`, not `execute` — in Sea-ORM 2.0 the un-suffixed one takes a
    // sea-query statement and builds the SQL itself.
    db.conn
        .execute_raw(Statement::from_sql_and_values(
            db.conn.get_database_backend(),
            "UPDATE stored_files SET staged_at = now() - interval '48 hours' WHERE key = $1",
            [stale.into()],
        ))
        .await
        .expect("age the abandoned row");

    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    let found = repo
        .attachment_keys_before(cutoff)
        .await
        .expect("attachment_keys_before");

    assert!(
        found.contains(&stale.to_string()),
        "a 48h-old unreferenced attachment must be reported; got {found:?}"
    );
    assert!(
        !found.contains(&fresh.to_string()),
        "a just-staged attachment must NOT be reported — that is somebody's open draft"
    );

    db.teardown().await;
}

#[tokio::test]
async fn refs_for_reports_count_and_age_and_omits_absent_keys() {
    // The sweep reads all three facts from this one query, and each drives a
    // different branch: a missing row is an orphan, `ref_count <= 0` is
    // uncollected, and `created_at` decides whether an attachment at zero is
    // offered for manual deletion or merely counted.
    let db = TestDb::new("sfile_refs_for").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    repo.upsert_and_ref("avatars/live.png", "image/png", 7, None)
        .await
        .expect("referenced row");
    repo.upsert_staged("post-attachments/staged.png", "image/png", 8, None)
        .await
        .expect("staged row");

    let before = chrono::Utc::now();
    let rows = repo
        .refs_for(&[
            "avatars/live.png".to_string(),
            "post-attachments/staged.png".to_string(),
            "avatars/no-such-row.png".to_string(),
        ])
        .await
        .expect("refs_for");

    assert_eq!(
        rows.len(),
        2,
        "a key with no row must be absent, not returned as a zero — that is how \
         the sweep tells an orphan from an uncollected row"
    );
    let by_key: std::collections::HashMap<_, _> =
        rows.into_iter().map(|r| (r.key.clone(), r)).collect();
    assert_eq!(by_key["avatars/live.png"].ref_count, 1);
    assert_eq!(by_key["post-attachments/staged.png"].ref_count, 0);
    for row in by_key.values() {
        // Both were written moments ago. A timezone mishandled on the way out
        // would land this hours away in either direction.
        let age = before - row.created_at;
        assert!(
            age < chrono::Duration::minutes(5) && age > chrono::Duration::minutes(-5),
            "created_at came back as {} against a now of {before}",
            row.created_at
        );
    }

    db.teardown().await;
}

#[tokio::test]
async fn refs_for_reports_an_aged_row_as_aged() {
    // The companion to the test above, and the one that matters for the sweep's
    // grace window: that one only proves a *fresh* row comes back as fresh, which
    // a sign error or a dropped offset would also satisfy. This one writes a row
    // two days into the past and checks `refs_for` says so — the difference
    // between "listed for manual review" and "silently counted as a live draft".
    let db = TestDb::new("sfile_refs_for_aged").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "post-attachments/two-days-old.png";

    repo.upsert_staged(key, "image/png", 5, None).await.expect("stage");
    db.conn
        .execute_raw(Statement::from_sql_and_values(
            db.conn.get_database_backend(),
            "UPDATE stored_files SET created_at = now() - interval '48 hours', \
                                     staged_at  = now() - interval '48 hours' \
             WHERE key = $1",
            [key.into()],
        ))
        .await
        .expect("age the row");

    // Both columns, because both cross a repository boundary as
    // `DateTime<FixedOffset>` and are converted by hand. Checking only one leaves
    // the other free to lose its offset, and `staged_at` is the one the grace
    // window reads.
    let rows = repo.refs_for(&[key.to_string()]).await.expect("refs_for");
    for (label, at) in [
        ("created_at", rows[0].created_at),
        ("staged_at", rows[0].staged_at),
    ] {
        let age = chrono::Utc::now() - at;
        assert!(
            age > chrono::Duration::hours(47) && age < chrono::Duration::hours(49),
            "{label} aged 48h in SQL came back {age} old — the sweep would treat this \
             row as a live draft"
        );
    }

    db.teardown().await;
}

#[tokio::test]
async fn re_staging_a_deduplicated_key_restarts_its_grace_period() {
    // THE regression test for the `staged_at` column.
    //
    // CAS keys are content digests, so re-uploading a byte-identical image lands
    // on the existing row — possibly one whose original post was deleted long ago
    // and whose `ref_count` is therefore 0. While both cleanup tools measured the
    // grace period from `created_at`, that row read as long-abandoned the instant
    // it was re-staged: the sweep offered it for manual deletion and the audit's
    // apply pass RELEASED it, deleting the image out of the composer that had just
    // uploaded it. Nothing about the sequence errors, and the loss is silent.
    let db = TestDb::new("sfile_restage_grace").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "post-attachments/reposted-meme.png";

    // A key from a post deleted long ago: old bytes, no references.
    repo.upsert_staged(key, "image/png", 6, None).await.expect("original stage");
    db.conn
        .execute_raw(Statement::from_sql_and_values(
            db.conn.get_database_backend(),
            "UPDATE stored_files SET created_at = now() - interval '400 days', \
                                     staged_at  = now() - interval '400 days' \
             WHERE key = $1",
            [key.into()],
        ))
        .await
        .expect("age the row into the distant past");

    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    assert_eq!(
        repo.attachment_keys_before(cutoff).await.expect("before"),
        vec![key.to_string()],
        "precondition: while nobody is using it, it is reportable"
    );

    // Somebody uploads the same image again — this is the whole scenario.
    repo.upsert_staged(key, "image/png", 6, None).await.expect("re-stage");

    assert!(
        repo.attachment_keys_before(cutoff)
            .await
            .expect("after")
            .is_empty(),
        "a just-re-staged attachment must be protected again — it is in an open composer"
    );

    let row = &repo.refs_for(&[key.to_string()]).await.expect("refs_for")[0];
    assert_eq!(row.ref_count, 0, "re-staging must not touch the reference count");
    assert!(
        chrono::Utc::now() - row.staged_at < chrono::Duration::minutes(5),
        "staged_at must move to now, got {}",
        row.staged_at
    );
    assert!(
        chrono::Utc::now() - row.created_at > chrono::Duration::days(399),
        "created_at must NOT move — it is the age of the bytes, and the quota and \
         the audit trail both read it"
    );

    db.teardown().await;
}

#[tokio::test]
async fn referencing_a_key_does_not_restart_its_grace_period() {
    // The other half: `upsert_and_ref` is the *publishing* path. Bumping
    // `staged_at` there would restart the grace period of an attachment a post is
    // adopting rather than abandoning — harmless today, since a referenced row is
    // filtered out by `ref_count` first, but it would silently shield the row for
    // 24h the moment that post were deleted.
    let db = TestDb::new("sfile_ref_keeps_staged_at").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());
    let key = "post-attachments/published.png";

    repo.upsert_staged(key, "image/png", 6, None).await.expect("stage");
    db.conn
        .execute_raw(Statement::from_sql_and_values(
            db.conn.get_database_backend(),
            "UPDATE stored_files SET staged_at = now() - interval '72 hours' WHERE key = $1",
            [key.into()],
        ))
        .await
        .expect("age");

    repo.upsert_and_ref(key, "image/png", 6, None).await.expect("publish");

    let row = &repo.refs_for(&[key.to_string()]).await.expect("refs_for")[0];
    let age = chrono::Utc::now() - row.staged_at;
    assert!(
        age > chrono::Duration::hours(71),
        "staged_at moved on the referencing path; age is {age}"
    );

    db.teardown().await;
}

#[tokio::test]
async fn staged_at_is_not_null_with_a_default() {
    // `upsert_and_ref` passes `staged_at: NotSet`, so the column DEFAULT is what
    // fills it on the insert path — and `refs_for` deserializes it into a
    // non-`Option`. If migration 000034's `modify_column` failed to apply either
    // property, the first would insert NULL and the second would then fail to
    // decode, on the hot path, for every upload.
    let db = TestDb::new("sfile_staged_at_shape").await;

    let row = db
        .conn
        .query_one_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT is_nullable, column_default FROM information_schema.columns \
             WHERE table_name = 'stored_files' AND column_name = 'staged_at'"
                .to_owned(),
        ))
        .await
        .expect("query information_schema")
        .expect("staged_at must exist");

    assert_eq!(
        row.try_get::<String>("", "is_nullable").expect("is_nullable"),
        "NO"
    );
    assert!(
        row.try_get::<Option<String>>("", "column_default")
            .expect("column_default")
            .is_some(),
        "no DEFAULT — `upsert_and_ref` inserts with NotSet and would write NULL"
    );

    db.teardown().await;
}

#[tokio::test]
async fn the_age_floor_query_actually_uses_its_partial_index() {
    // Migration 000034 creates `idx_stored_files_staged_at` partial on
    // `key LIKE 'post-attachments/%'`, and its comment claims the audit's age
    // query is an index range scan rather than a table scan. That claim was
    // written without checking, and it is exactly the kind that fails silently:
    // Postgres uses a partial index only when the query predicate *implies* the
    // index predicate, so a change in how the prefix filter is spelled costs a
    // sequential scan of `stored_files` on every audit, with no error anywhere.
    //
    // `enable_seqscan = off` does not force an index — it prices sequential scans
    // absurdly high. A plan that still seq-scans under it cannot use the index at
    // all, which is the failure being ruled out. The same technique guards
    // `idx_posts_fts` in the search service.
    let db = TestDb::new("sfile_staged_at_index").await;
    let repo = PgStoredFileRepository::new(db.conn.clone());

    // A few rows so the planner has something to reason about.
    for i in 0..20 {
        repo.upsert_staged(
            &format!("post-attachments/{i:032x}.png"),
            "image/png",
            4,
            None,
        )
        .await
        .expect("stage");
    }

    // The query `attachment_keys_before` builds, taken from the same constant its
    // filter uses so the prefix cannot drift between them. The shape (`SELECT key
    // … WHERE key LIKE $prefix% AND staged_at < …`) is mirrored here rather than
    // captured, which is this test's one weakness: it proves the predicate *can*
    // use the index, not that the repository still writes that predicate.
    let sql = format!(
        "EXPLAIN SELECT key FROM stored_files \
         WHERE key LIKE '{ATTACHMENT_KEY_PREFIX}%' AND staged_at < now() ORDER BY key",
    );

    db.conn
        .execute_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SET enable_seqscan = off".to_owned(),
        ))
        .await
        .expect("disable seqscan");

    let rows = db
        .conn
        .query_all_raw(Statement::from_string(sea_orm::DbBackend::Postgres, sql))
        .await
        .expect("EXPLAIN");
    let plan = rows
        .iter()
        .map(|r| r.try_get::<String>("", "QUERY PLAN").unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");

    // Three claims, because the weakest of them passes on its own — measured, not
    // assumed. Pointing the index at `size` while keeping its name leaves the
    // partial predicate satisfied, so Postgres still scans it end to end and the
    // plan still *names* it; only the age bound quietly degrades from an index
    // condition to a per-row filter. A name-only assertion passed that mutation.
    assert!(
        !plan.contains("Seq Scan"),
        "the age floor query falls back to a table scan. Plan was:\n{plan}"
    );
    assert!(
        plan.contains("Index Scan using idx_stored_files_staged_at"),
        "the partial index is not used at all. Plan was:\n{plan}"
    );
    assert!(
        plan.contains("Index Cond: (staged_at"),
        "the index is scanned but the age bound is only a Filter, so every \
         attachment row is read and discarded. Plan was:\n{plan}"
    );

    db.teardown().await;
}
