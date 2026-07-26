//! Integration tests for [`PgPermissionRepository`].
//!
//! Permissions are seeded by PgSystemSeedService — `list_all` runs immediately against
//! the migrated schema. `set_role_permissions` replaces the full permission set
//! for a role (DELETE + INSERT in a transaction).

use ferum_domain::repositories::permission_repository::PermissionRepository;
use ferum_infrastructure::repositories::PgPermissionRepository;

use crate::common::{insert_role, TestDb};

#[tokio::test]
async fn list_all_returns_seeded_permissions() {
    // The system seeder writes the canonical permission rows; list_all must return them.
    let db = TestDb::new("perm_list_all").await;
    let repo = PgPermissionRepository::new(db.conn.clone());
    let perms = repo.list_all().await.expect("list_all");
    assert!(!perms.is_empty(), "migrations must seed at least one permission");
    // Spot-check a well-known key
    let has_thread_create = perms.iter().any(|p| p.key == "thread.create");
    assert!(has_thread_create, "thread.create permission must be seeded");
    db.teardown().await;
}

#[tokio::test]
async fn list_for_role_on_role_with_no_perms_returns_empty() {
    let db = TestDb::new("perm_list_for_role_empty").await;
    let role = insert_role(&db.conn, "norole").await;
    let repo = PgPermissionRepository::new(db.conn.clone());
    let perms = repo.list_for_role(role.id).await.expect("list_for_role");
    assert!(perms.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn set_role_permissions_replaces_existing_set() {
    let db = TestDb::new("perm_set_role_perms").await;
    let role = insert_role(&db.conn, "custom").await;
    let repo = PgPermissionRepository::new(db.conn.clone());

    // Initial assignment
    repo.set_role_permissions(role.id, &["thread.create".to_string()]).await.expect("set initial");
    let perms = repo.list_for_role(role.id).await.expect("list");
    assert_eq!(perms.len(), 1);
    assert_eq!(perms[0].key, "thread.create");

    // Replace with different set
    repo.set_role_permissions(role.id, &[
        "post.create".to_string(),
        "reaction.add".to_string(),
    ]).await.expect("set replacement");

    let perms2 = repo.list_for_role(role.id).await.expect("list after replace");
    assert_eq!(perms2.len(), 2);
    let keys: Vec<&str> = perms2.iter().map(|p| p.key.as_str()).collect();
    assert!(keys.contains(&"post.create"));
    assert!(keys.contains(&"reaction.add"));
    assert!(!keys.contains(&"thread.create"), "old perm should be removed after replacement");
    db.teardown().await;
}

#[tokio::test]
async fn set_role_permissions_with_empty_list_clears_all() {
    let db = TestDb::new("perm_set_role_perms_clear").await;
    let role = insert_role(&db.conn, "temp").await;
    let repo = PgPermissionRepository::new(db.conn.clone());

    repo.set_role_permissions(role.id, &["thread.create".to_string()]).await.expect("set");
    repo.set_role_permissions(role.id, &[]).await.expect("clear");

    let perms = repo.list_for_role(role.id).await.expect("list after clear");
    assert!(perms.is_empty());
    db.teardown().await;
}
