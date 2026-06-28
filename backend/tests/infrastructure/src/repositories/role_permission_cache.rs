//! Integration tests for [`RolePermissionCache`].
//!
//! `load()` uses a LEFT JOIN + `min_trust::text` cast in a `PostgresQueryBuilder` query
//! — the same class of raw SQL that triggered bugs in other repositories.
//! The in-memory resolution methods are also tested here.

use ferum_domain::models::role::UserRoleAssignment;
use ferum_domain::repositories::{
    permission_repository::PermissionRepository,
    user_role_repository::UserRoleRepository,
};
use ferum_infrastructure::{
    repositories::{PgPermissionRepository, PgUserRoleRepository},
    role_permission_cache::RolePermissionCache,
};

use crate::common::{insert_role, insert_user, insert_category, TestDb};

// ─── load() ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn load_succeeds_on_fresh_schema() {
    // Migrations seed permissions; LEFT JOIN must not error on first load.
    let db = TestDb::new("rpc_load_fresh").await;
    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load() on fresh migrated DB");
    db.teardown().await;
}

#[tokio::test]
async fn load_populates_min_trust_map() {
    let db = TestDb::new("rpc_load_min_trust").await;
    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load");

    // After load, resolve_min_trust must return all seeded permissions
    let trust_map = cache.resolve_min_trust().await.expect("resolve_min_trust");
    assert!(trust_map.contains_key("thread.create"), "thread.create must be in trust map");
    assert!(!trust_map.is_empty(), "at least one permission must be loaded");
    db.teardown().await;
}

#[tokio::test]
async fn load_then_resolve_global_for_role_with_permissions() {
    let db = TestDb::new("rpc_resolve_global").await;
    let role = insert_role(&db.conn, "editor").await;
    let user = insert_user(&db.conn, 1).await;
    let granter = insert_user(&db.conn, 2).await;

    // Assign thread.create to the role
    let perm_repo = PgPermissionRepository::new(db.conn.clone());
    perm_repo.set_role_permissions(role.id, &["thread.create".to_string()])
        .await.expect("set_role_permissions");

    // Assign the role globally to user
    PgUserRoleRepository::new(db.conn.clone())
        .assign(user.id, role.id, None, granter.id, None)
        .await.expect("assign role");

    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load");

    let assignment = UserRoleAssignment {
        id: uuid::Uuid::new_v4(),
        user_id: user.id,
        role_id: role.id,
        role_slug: "editor".to_string(),
        role_name: "editor role".to_string(),
        role_color: None,
        category_id: None,
        granted_by: Some(granter.id),
        expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let perms = cache.resolve_global(&[assignment]).await;
    assert!(perms.contains("thread.create"), "global assignment must resolve thread.create");
    db.teardown().await;
}

#[tokio::test]
async fn resolve_global_ignores_category_scoped_assignments() {
    let db = TestDb::new("rpc_global_ignores_cat").await;
    let role = insert_role(&db.conn, "mod").await;
    let cat = insert_category(&db.conn, 1).await;
    let user = insert_user(&db.conn, 1).await;

    let perm_repo = PgPermissionRepository::new(db.conn.clone());
    perm_repo.set_role_permissions(role.id, &["thread.lock".to_string()])
        .await.expect("set_role_permissions");

    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load");

    // category-scoped assignment — must NOT appear in global resolution
    let assignment = UserRoleAssignment {
        id: uuid::Uuid::new_v4(),
        user_id: user.id,
        role_id: role.id,
        role_slug: "mod".to_string(),
        role_name: "mod role".to_string(),
        role_color: None,
        category_id: Some(cat.id),
        granted_by: None,
        expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let perms = cache.resolve_global(&[assignment]).await;
    assert!(perms.is_empty(), "category-scoped assignments must not appear in global resolution");
    db.teardown().await;
}

#[tokio::test]
async fn resolve_category_ignores_global_assignments() {
    let db = TestDb::new("rpc_cat_ignores_global").await;
    let role = insert_role(&db.conn, "member").await;
    let user = insert_user(&db.conn, 1).await;

    let perm_repo = PgPermissionRepository::new(db.conn.clone());
    perm_repo.set_role_permissions(role.id, &["post.create".to_string()])
        .await.expect("set_role_permissions");

    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load");

    // global assignment — must NOT appear in category resolution
    let assignment = UserRoleAssignment {
        id: uuid::Uuid::new_v4(),
        user_id: user.id,
        role_id: role.id,
        role_slug: "member".to_string(),
        role_name: "member role".to_string(),
        role_color: None,
        category_id: None,
        granted_by: None,
        expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let cat_perms = cache.resolve_category(&[assignment]).await;
    assert!(cat_perms.is_empty(), "global assignments must not appear in category resolution");
    db.teardown().await;
}

#[tokio::test]
async fn resolve_category_groups_by_category_id() {
    let db = TestDb::new("rpc_cat_grouped").await;
    let role = insert_role(&db.conn, "mod").await;
    let cat_a = insert_category(&db.conn, 1).await;
    let cat_b = insert_category(&db.conn, 2).await;
    let user = insert_user(&db.conn, 1).await;

    let perm_repo = PgPermissionRepository::new(db.conn.clone());
    perm_repo.set_role_permissions(role.id, &["thread.lock".to_string()])
        .await.expect("set_role_permissions");

    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load");

    let a1 = UserRoleAssignment { id: uuid::Uuid::new_v4(), user_id: user.id, role_id: role.id, role_slug: "mod".to_string(), role_name: "mod role".to_string(), role_color: None, category_id: Some(cat_a.id), granted_by: None, expires_at: None, created_at: chrono::Utc::now() };
    let a2 = UserRoleAssignment { id: uuid::Uuid::new_v4(), user_id: user.id, role_id: role.id, role_slug: "mod".to_string(), role_name: "mod role".to_string(), role_color: None, category_id: Some(cat_b.id), granted_by: None, expires_at: None, created_at: chrono::Utc::now() };

    let cat_perms = cache.resolve_category(&[a1, a2]).await;
    assert!(cat_perms[&cat_a.id].contains("thread.lock"));
    assert!(cat_perms[&cat_b.id].contains("thread.lock"));
    db.teardown().await;
}

#[tokio::test]
async fn permissions_for_role_returns_assigned_keys() {
    let db = TestDb::new("rpc_perms_for_role").await;
    let role = insert_role(&db.conn, "writer").await;

    let perm_repo = PgPermissionRepository::new(db.conn.clone());
    perm_repo.set_role_permissions(role.id, &[
        "thread.create".to_string(),
        "post.create".to_string(),
    ]).await.expect("set_role_permissions");

    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("load");

    let mut keys = cache.permissions_for_role(role.id).await;
    keys.sort();
    assert_eq!(keys, vec!["post.create", "thread.create"]);
    db.teardown().await;
}

#[tokio::test]
async fn reload_refreshes_cache_after_permission_change() {
    let db = TestDb::new("rpc_reload_refresh").await;
    let role = insert_role(&db.conn, "evolving").await;

    let perm_repo = PgPermissionRepository::new(db.conn.clone());
    perm_repo.set_role_permissions(role.id, &["thread.create".to_string()])
        .await.expect("set initial");

    let cache = RolePermissionCache::new(db.conn.clone());
    cache.load().await.expect("initial load");

    let keys_before = cache.permissions_for_role(role.id).await;
    assert!(keys_before.contains(&"thread.create".to_string()));

    // Admin changes permissions
    perm_repo.set_role_permissions(role.id, &["post.create".to_string()])
        .await.expect("update perms");

    // Cache is stale until reload()
    let cache_stale = cache.permissions_for_role(role.id).await;
    assert!(cache_stale.contains(&"thread.create".to_string()), "still stale before reload");

    cache.load().await.expect("reload");

    let keys_after = cache.permissions_for_role(role.id).await;
    assert!(keys_after.contains(&"post.create".to_string()), "should have new perm after reload");
    assert!(!keys_after.contains(&"thread.create".to_string()), "old perm removed after reload");
    db.teardown().await;
}
