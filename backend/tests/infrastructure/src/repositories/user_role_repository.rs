//! Integration tests for [`PgUserRoleRepository`].
//!
//! Critical SQL:
//!   - `list_for_user`: custom SELECT with `expires_at > now()` correlated filter
//!   - `list_for_users`: batch N-user query
//!   - `count_by_role`: `COUNT(DISTINCT user_id)::BIGINT` + GROUP BY
//!   - `count_global_by_role`: same but `WHERE category_id IS NULL`

use ferum_domain::repositories::user_role_repository::UserRoleRepository;
use ferum_infrastructure::repositories::PgUserRoleRepository;

use crate::common::{insert_category, insert_role, insert_user, TestDb};

#[tokio::test]
async fn list_for_user_on_empty_schema_returns_empty() {
    let db = TestDb::new("ura_list_user_empty").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgUserRoleRepository::new(db.conn.clone());
    let assignments = repo.list_for_user(user.id).await.expect("list_for_user");
    assert!(assignments.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn assign_global_and_list_for_user() {
    let db = TestDb::new("ura_assign_global").await;
    let user = insert_user(&db.conn, 1).await;
    let granter = insert_user(&db.conn, 2).await;
    let role = insert_role(&db.conn, "member").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    repo.assign(user.id, role.id, None, granter.id, None).await.expect("assign");

    let assignments = repo.list_for_user(user.id).await.expect("list_for_user");
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].role_id, role.id);
    assert!(assignments[0].category_id.is_none(), "global assignment has no category_id");
    db.teardown().await;
}

#[tokio::test]
async fn assign_category_scoped_role() {
    let db = TestDb::new("ura_assign_cat_scoped").await;
    let user = insert_user(&db.conn, 1).await;
    let granter = insert_user(&db.conn, 2).await;
    let cat = insert_category(&db.conn, 1).await;
    let role = insert_role(&db.conn, "moderator").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    repo.assign(user.id, role.id, Some(cat.id), granter.id, None).await.expect("assign");

    let assignments = repo.list_for_user(user.id).await.expect("list_for_user");
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].category_id, Some(cat.id));
    db.teardown().await;
}

#[tokio::test]
async fn revoke_removes_assignment() {
    let db = TestDb::new("ura_revoke").await;
    let user = insert_user(&db.conn, 1).await;
    let granter = insert_user(&db.conn, 2).await;
    let role = insert_role(&db.conn, "member").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    repo.assign(user.id, role.id, None, granter.id, None).await.expect("assign");
    repo.revoke(user.id, role.id, None).await.expect("revoke");

    let assignments = repo.list_for_user(user.id).await.expect("list_for_user");
    assert!(assignments.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn list_for_users_batch_query() {
    // Validates the batch SELECT path (no N+1).
    let db = TestDb::new("ura_list_for_users_batch").await;
    let user_a = insert_user(&db.conn, 1).await;
    let user_b = insert_user(&db.conn, 2).await;
    let granter = insert_user(&db.conn, 3).await;
    let role = insert_role(&db.conn, "member").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    repo.assign(user_a.id, role.id, None, granter.id, None).await.expect("assign a");
    repo.assign(user_b.id, role.id, None, granter.id, None).await.expect("assign b");

    let result = repo.list_for_users(&[user_a.id, user_b.id]).await.expect("list_for_users");
    assert_eq!(result.len(), 2);
    db.teardown().await;
}

#[tokio::test]
async fn list_for_users_empty_ids_returns_empty() {
    let db = TestDb::new("ura_list_for_users_empty").await;
    let repo = PgUserRoleRepository::new(db.conn.clone());
    let result = repo.list_for_users(&[]).await.expect("list_for_users");
    assert!(result.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn count_by_role_deduplicates_users() {
    // `COUNT(DISTINCT user_id)::BIGINT` — user assigned both globally and per-category
    // should count as 1.
    let db = TestDb::new("ura_count_dedup").await;
    let user = insert_user(&db.conn, 1).await;
    let granter = insert_user(&db.conn, 2).await;
    let cat = insert_category(&db.conn, 1).await;
    let role = insert_role(&db.conn, "mod").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    repo.assign(user.id, role.id, None, granter.id, None).await.expect("global assign");
    repo.assign(user.id, role.id, Some(cat.id), granter.id, None).await.expect("cat assign");

    let counts = repo.count_by_role().await.expect("count_by_role");
    let count = counts.get(&role.id).copied().unwrap_or(0);
    assert_eq!(count, 1, "same user in multiple assignments should count as 1");
    db.teardown().await;
}

#[tokio::test]
async fn count_global_by_role_excludes_category_scoped() {
    // `WHERE category_id IS NULL` must exclude category-scoped assignments.
    let db = TestDb::new("ura_count_global").await;
    let user_a = insert_user(&db.conn, 1).await;
    let user_b = insert_user(&db.conn, 2).await;
    let granter = insert_user(&db.conn, 3).await;
    let cat = insert_category(&db.conn, 1).await;
    let role = insert_role(&db.conn, "admin").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    // user_a has global assignment
    repo.assign(user_a.id, role.id, None, granter.id, None).await.expect("global");
    // user_b has only category-scoped assignment
    repo.assign(user_b.id, role.id, Some(cat.id), granter.id, None).await.expect("cat-scoped");

    let counts = repo.count_global_by_role().await.expect("count_global_by_role");
    let count = counts.get(&role.id).copied().unwrap_or(0);
    assert_eq!(count, 1, "only global (category_id IS NULL) assignments counted");
    db.teardown().await;
}

#[tokio::test]
async fn list_for_category_returns_scoped_assignments() {
    let db = TestDb::new("ura_list_for_category").await;
    let user = insert_user(&db.conn, 1).await;
    let granter = insert_user(&db.conn, 2).await;
    let cat = insert_category(&db.conn, 1).await;
    let role = insert_role(&db.conn, "mod").await;

    let repo = PgUserRoleRepository::new(db.conn.clone());
    repo.assign(user.id, role.id, Some(cat.id), granter.id, None).await.expect("assign");

    let result = repo.list_for_category(role.id, Some(cat.id)).await.expect("list_for_category");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].user_id, user.id);
    db.teardown().await;
}
