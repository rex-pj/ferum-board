//! Integration tests for [`PgRoleRepository`].

use ferum_domain::repositories::role_repository::{NewRole, RoleRepository, UpdateRole};
use ferum_infrastructure::repositories::PgRoleRepository;

use crate::common::TestDb;

fn new_role(slug: &str) -> NewRole {
    NewRole {
        slug: slug.to_string(),
        name: format!("{slug} role"),
        description: Some(format!("The {slug} description")),
        color: None,
        position: 0,
    }
}

#[tokio::test]
async fn list_returns_seeded_system_roles() {
    // Migrations seed admin / moderator / member as system roles.
    let db = TestDb::new("role_list_empty").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    let roles = repo.list().await.expect("list");
    assert_eq!(roles.len(), 3);
    assert!(roles.iter().all(|r| r.is_system));
    db.teardown().await;
}

#[tokio::test]
async fn create_and_find_by_slug() {
    let db = TestDb::new("role_create_find_slug").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    let role = repo.create(new_role("test-moderator")).await.expect("create");

    let found = repo.find_by_slug("test-moderator").await.expect("find_by_slug").unwrap();
    assert_eq!(found.id, role.id);
    assert_eq!(found.slug, "test-moderator");
    db.teardown().await;
}

#[tokio::test]
async fn find_by_slug_missing_returns_none() {
    let db = TestDb::new("role_find_slug_none").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    assert!(repo.find_by_slug("ghost").await.expect("find_by_slug").is_none());
    db.teardown().await;
}

#[tokio::test]
async fn find_by_id_returns_role() {
    let db = TestDb::new("role_find_by_id").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    let role = repo.create(new_role("test-member")).await.expect("create");
    let found = repo.find_by_id(role.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.id, role.id);
    db.teardown().await;
}

#[tokio::test]
async fn update_role_patches_name_and_color() {
    let db = TestDb::new("role_update").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    let role = repo.create(new_role("custom")).await.expect("create");

    let updated = repo.update(role.id, UpdateRole {
        name: Some("Custom Updated".to_string()),
        color: Some(Some("#ff0000".to_string())),
        description: None,
        position: None,
    }).await.expect("update");

    assert_eq!(updated.name, "Custom Updated");
    assert_eq!(updated.color.as_deref(), Some("#ff0000"));
    db.teardown().await;
}

#[tokio::test]
async fn delete_non_system_role_succeeds() {
    let db = TestDb::new("role_delete").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    let role = repo.create(new_role("temp")).await.expect("create");
    repo.delete(role.id).await.expect("delete");
    assert!(repo.find_by_id(role.id).await.expect("find_by_id").is_none());
    db.teardown().await;
}

#[tokio::test]
async fn list_returns_all_created_roles() {
    // Migrations seed 3 system roles; we add 2 more and verify they appear.
    let db = TestDb::new("role_list_all").await;
    let repo = PgRoleRepository::new(db.conn.clone());
    repo.create(new_role("r1")).await.expect("create r1");
    repo.create(new_role("r2")).await.expect("create r2");
    let list = repo.list().await.expect("list");
    let slugs: Vec<&str> = list.iter().map(|r| r.slug.as_str()).collect();
    assert!(slugs.contains(&"r1"), "r1 must appear in list");
    assert!(slugs.contains(&"r2"), "r2 must appear in list");
    db.teardown().await;
}
