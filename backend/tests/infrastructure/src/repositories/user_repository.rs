//! Integration tests for [`PgUserRepository`].
//!
//! These tests exercise every raw-SQL fragment the compiler cannot check:
//!   - `trust_level::text` enum cast in `user_select()`
//!   - Correlated sub-select for `primary_role_slug`
//!   - `RETURNING` clause in `increment_failed_login`
//!   - `COUNT(DISTINCT user_id)::BIGINT` in `count_admins`
//!   - `DATE(last_seen_at AT TIME ZONE 'UTC') < CURRENT_DATE` CASE expression
//!   - `GREATEST` / `LEAST` in `increment_post_count` / `increment_trust_score`
//!   - `ON CONFLICT` upsert in `upsert_preferences`

use ferum_domain::models::user::TrustLevel;
use ferum_domain::repositories::user_repository::{NewUser, UpdateUser, UserRepository};
use ferum_infrastructure::repositories::PgUserRepository;

use crate::common::TestDb;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn new_user(n: u8) -> NewUser {
    NewUser {
        username: format!("user{n}"),
        email: format!("user{n}@example.com"),
        password_hash: Some("$2b$12$fakehash".to_string()),
    }
}

// ─── Tests: read-only on empty schema ─────────────────────────────────────────

#[tokio::test]
async fn find_by_id_on_empty_schema_returns_none() {
    let db = TestDb::new("user_find_by_id_empty").await;
    let repo = PgUserRepository::new(db.conn.clone());

    // user_select() contains a correlated sub-select and a trust_level::text cast;
    // both must execute without syntax errors against the live schema.
    let result = repo
        .find_by_id(uuid::Uuid::new_v4())
        .await
        .expect("user_select() runs on empty schema");
    assert!(result.is_none());

    db.teardown().await;
}

#[tokio::test]
async fn list_paginated_on_empty_schema_returns_zero() {
    let db = TestDb::new("user_list_paginated_empty").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let (users, total) = repo
        .list_paginated(1, 20, None, None, None)
        .await
        .expect("list_paginated executes on empty schema");
    assert_eq!(total, 0);
    assert!(users.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn list_paginated_search_filter_executes() {
    let db = TestDb::new("user_list_paginated_search").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let (users, total) = repo
        .list_paginated(1, 20, Some("alice"), Some("username"), Some("asc"))
        .await
        .expect("list_paginated with ILIKE search executes");
    assert_eq!(total, 0);
    assert!(users.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn count_admins_on_empty_schema_returns_zero() {
    let db = TestDb::new("user_count_admins_empty").await;
    let repo = PgUserRepository::new(db.conn.clone());

    // COUNT(DISTINCT user_id)::BIGINT with inner JOIN to roles — must not error.
    let count = repo
        .count_admins()
        .await
        .expect("count_admins runs COUNT(DISTINCT)::BIGINT on empty schema");
    assert_eq!(count, 0);

    db.teardown().await;
}

// ─── Tests: create → find roundtrip ───────────────────────────────────────────

#[tokio::test]
async fn create_and_find_by_email_returns_user_with_trust_level_cast() {
    let db = TestDb::new("user_create_find_email").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    // find_by_email re-uses user_select() which casts trust_level::text
    let found = repo
        .find_by_email("user1@example.com")
        .await
        .expect("find_by_email executes trust_level::text cast")
        .expect("user should exist");

    assert_eq!(found.id, user.id);
    assert_eq!(found.username, "user1");
    assert!(matches!(found.trust_level, TrustLevel::New));
    assert!(found.primary_role_slug.is_none()); // no roles assigned yet

    db.teardown().await;
}

#[tokio::test]
async fn create_and_find_by_username_returns_user() {
    let db = TestDb::new("user_create_find_username").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(2)).await.expect("create user");
    let found = repo
        .find_by_username("user2")
        .await
        .expect("find_by_username executes")
        .expect("user should exist");

    assert_eq!(found.id, user.id);

    db.teardown().await;
}

#[tokio::test]
async fn find_many_by_ids_returns_correct_subset() {
    let db = TestDb::new("user_find_many_by_ids").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let u1 = repo.create(new_user(1)).await.expect("create u1");
    let u2 = repo.create(new_user(2)).await.expect("create u2");
    repo.create(new_user(3)).await.expect("create u3");

    let found = repo
        .find_many_by_ids(&[u1.id, u2.id])
        .await
        .expect("find_many_by_ids executes");

    assert_eq!(found.len(), 2);

    db.teardown().await;
}

#[tokio::test]
async fn set_trust_level_and_read_back() {
    let db = TestDb::new("user_set_trust_level").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    for level in [
        TrustLevel::Basic,
        TrustLevel::Member,
        TrustLevel::Regular,
        TrustLevel::Leader,
        TrustLevel::New,
    ] {
        repo.set_trust_level(user.id, level.clone())
            .await
            .expect("set_trust_level executes");
        let found = repo
            .find_by_id(user.id)
            .await
            .expect("find_by_id executes")
            .expect("user should exist");
        assert!(
            std::mem::discriminant(&found.trust_level) == std::mem::discriminant(&level),
            "trust_level mismatch after setting {level:?}"
        );
    }

    db.teardown().await;
}

#[tokio::test]
async fn increment_failed_login_returning_clause_works() {
    let db = TestDb::new("user_incr_failed_login").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    // increment_failed_login uses a raw UPDATE … RETURNING query
    let c1 = repo
        .increment_failed_login(user.id)
        .await
        .expect("increment_failed_login with RETURNING");
    assert_eq!(c1, 1);

    let c2 = repo
        .increment_failed_login(user.id)
        .await
        .expect("second increment");
    assert_eq!(c2, 2);

    db.teardown().await;
}

#[tokio::test]
async fn reset_failed_login_zeroes_count() {
    let db = TestDb::new("user_reset_failed_login").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");
    repo.increment_failed_login(user.id).await.expect("incr");
    repo.increment_failed_login(user.id).await.expect("incr");

    repo.reset_failed_login(user.id)
        .await
        .expect("reset_failed_login executes");

    let found = repo
        .find_by_id(user.id)
        .await
        .expect("find_by_id")
        .expect("user should exist");
    assert_eq!(found.failed_login_count, 0);

    db.teardown().await;
}

#[tokio::test]
async fn update_last_seen_date_case_expression_runs() {
    let db = TestDb::new("user_update_last_seen").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    // update_last_seen uses:
    //   DATE(last_seen_at AT TIME ZONE 'UTC') < CURRENT_DATE
    // and a CASE statement — both must execute without error.
    repo.update_last_seen(user.id)
        .await
        .expect("update_last_seen DATE/CASE SQL executes");

    let found = repo
        .find_by_id(user.id)
        .await
        .expect("find_by_id")
        .expect("user should exist");
    assert!(found.last_seen_at.is_some());
    assert_eq!(found.days_visited, 1);

    // Second call same day must NOT increment days_visited again
    repo.update_last_seen(user.id)
        .await
        .expect("second update_last_seen");
    let found2 = repo
        .find_by_id(user.id)
        .await
        .expect("find_by_id")
        .expect("user should exist");
    assert_eq!(found2.days_visited, 1, "days_visited must not double-count same-day visits");

    db.teardown().await;
}

#[tokio::test]
async fn increment_post_count_greatest_prevents_negative() {
    let db = TestDb::new("user_incr_post_count").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    // GREATEST(0, post_count + delta) must floor at 0
    repo.increment_post_count(user.id, -10)
        .await
        .expect("increment_post_count with negative delta");

    let found = repo
        .find_by_id(user.id)
        .await
        .expect("find_by_id")
        .expect("user should exist");
    assert_eq!(found.post_count, 0, "GREATEST(0, …) must prevent negative post_count");

    repo.increment_post_count(user.id, 5).await.expect("incr +5");
    let found2 = repo
        .find_by_id(user.id)
        .await
        .expect("find_by_id")
        .expect("user should exist");
    assert_eq!(found2.post_count, 5);

    db.teardown().await;
}

#[tokio::test]
async fn increment_trust_score_clamps_between_0_and_100() {
    let db = TestDb::new("user_incr_trust_score").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    // GREATEST(0, LEAST(100, trust_score + amount))
    repo.increment_trust_score(user.id, 200)
        .await
        .expect("increment_trust_score capped at 100");
    let found = repo.find_by_id(user.id).await.expect("find").expect("user");
    assert_eq!(found.trust_score, 100, "LEAST(100, …) must cap at 100");

    repo.increment_trust_score(user.id, -200)
        .await
        .expect("increment_trust_score floored at 0");
    let found2 = repo.find_by_id(user.id).await.expect("find").expect("user");
    assert_eq!(found2.trust_score, 0, "GREATEST(0, …) must floor at 0");

    db.teardown().await;
}

#[tokio::test]
async fn preferences_upsert_is_idempotent() {
    use ferum_domain::models::user::UserPreferences;

    let db = TestDb::new("user_prefs_upsert").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    // First read: default preferences (no DB row yet)
    let prefs = repo
        .get_preferences(user.id)
        .await
        .expect("get_preferences on missing row returns defaults");
    assert_eq!(prefs.theme, UserPreferences::default().theme);

    // Upsert: insert
    let mut updated = prefs.clone();
    updated.theme = "dark".to_string();
    repo.upsert_preferences(updated.clone())
        .await
        .expect("upsert_preferences inserts");

    // Upsert: ON CONFLICT update
    updated.theme = "light".to_string();
    repo.upsert_preferences(updated.clone())
        .await
        .expect("upsert_preferences updates on conflict");

    let final_prefs = repo
        .get_preferences(user.id)
        .await
        .expect("get_preferences reads updated row");
    assert_eq!(final_prefs.theme, "light");

    db.teardown().await;
}

#[tokio::test]
async fn update_partial_patch_only_changes_provided_fields() {
    let db = TestDb::new("user_update_partial").await;
    let repo = PgUserRepository::new(db.conn.clone());

    let user = repo.create(new_user(1)).await.expect("create user");

    let patched = repo
        .update(
            user.id,
            UpdateUser {
                display_name: Some(Some("Alice".to_string())),
                bio: None,
                ..Default::default()
            },
        )
        .await
        .expect("update partial patch executes");

    assert_eq!(patched.display_name.as_deref(), Some("Alice"));
    assert!(patched.bio.is_none(), "bio should remain None");

    db.teardown().await;
}
