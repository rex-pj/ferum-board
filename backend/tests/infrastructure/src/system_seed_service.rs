//! Integration tests for [`PgSystemSeedService`].
//!
//! The harness already seeds every test database once (see `common::TestDb`), so
//! these tests assert on that result and then re-run the seeder to prove the
//! properties that matter operationally: it is idempotent, and it does not
//! reclaim ground an admin has deliberately taken away.

use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult, Statement};

use ferum_domain::models::role::PERMISSIONS;
use ferum_infrastructure::system_seed_service::PgSystemSeedService;

use crate::common::TestDb;

#[derive(Debug, FromQueryResult)]
struct Count {
    n: i64,
}

async fn count(conn: &DatabaseConnection, sql: &str) -> i64 {
    Count::find_by_statement(Statement::from_string(conn.get_database_backend(), sql.to_owned()))
        .one(conn)
        .await
        .expect("count query")
        .expect("count row")
        .n
}

#[tokio::test]
async fn seeds_every_defined_permission_and_the_system_roles() {
    let db = TestDb::new("sys_seed_baseline").await;

    assert_eq!(
        count(&db.conn, "SELECT count(*)::bigint AS n FROM permissions").await,
        PERMISSIONS.len() as i64,
        "every PERMISSIONS entry must exist as a row"
    );
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM roles WHERE slug IN ('admin','moderator','member')"
        )
        .await,
        3
    );
    // Admin holds everything — that is `RoleGrants::All`, and it is what keeps a
    // newly added permission reachable without a migration.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM role_permissions rp \
             JOIN roles r ON r.id = rp.role_id WHERE r.slug = 'admin'"
        )
        .await,
        PERMISSIONS.len() as i64
    );
    // The default role must exist and be exactly one, or registration has no
    // role to hand out (or picks arbitrarily between two).
    assert_eq!(
        count(&db.conn, "SELECT count(*)::bigint AS n FROM roles WHERE is_default").await,
        1
    );

    db.teardown().await;
}

#[tokio::test]
async fn seeds_config_defaults_theme_and_catalogue_taxonomy() {
    let db = TestDb::new("sys_seed_side_tables").await;

    // Spot-check a key that used to be missing from the migration's list, so the
    // admin settings form rendered it blank.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM site_config \
             WHERE key = 'post_edit_window_hours' AND value = '24'"
        )
        .await,
        1
    );
    // Locale keys are deliberately NOT seeded: an absent `enabled_locales` means
    // "every installed locale", which no value can express.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM site_config WHERE key = 'enabled_locales'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM themes WHERE slug = 'default' AND is_active AND is_system"
        )
        .await,
        1
    );
    assert_eq!(
        count(&db.conn, "SELECT count(*)::bigint AS n FROM product_categories").await,
        7
    );

    db.teardown().await;
}

#[tokio::test]
async fn is_idempotent_across_repeated_runs() {
    let db = TestDb::new("sys_seed_idempotent").await;
    let seeder = PgSystemSeedService::new(db.conn.clone());

    let before = (
        count(&db.conn, "SELECT count(*)::bigint AS n FROM permissions").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM roles").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM role_permissions").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM site_config").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM themes").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM product_categories").await,
    );

    // Twice, because this runs on every startup — the second boot is the common
    // case, not the exception.
    seeder.seed_system().await.expect("second run");
    seeder.seed_system().await.expect("third run");

    let after = (
        count(&db.conn, "SELECT count(*)::bigint AS n FROM permissions").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM roles").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM role_permissions").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM site_config").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM themes").await,
        count(&db.conn, "SELECT count(*)::bigint AS n FROM product_categories").await,
    );

    assert_eq!(before, after, "re-running the seeder must change nothing");
    db.teardown().await;
}

#[tokio::test]
async fn does_not_restore_a_revoked_grant() {
    let db = TestDb::new("sys_seed_keeps_revocation").await;

    // What an admin does at /admin/permissions: take one permission away from
    // the member role.
    db.conn
        .execute_unprepared(
            "DELETE FROM role_permissions rp \
             USING roles r, permissions p \
             WHERE rp.role_id = r.id AND rp.permission_id = p.id \
               AND r.slug = 'member' AND p.key = 'thread.create'",
        )
        .await
        .expect("revoke");

    PgSystemSeedService::new(db.conn.clone())
        .seed_system()
        .await
        .expect("reseed after revocation");

    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM role_permissions rp \
             JOIN roles r ON r.id = rp.role_id \
             JOIN permissions p ON p.id = rp.permission_id \
             WHERE r.slug = 'member' AND p.key = 'thread.create'"
        )
        .await,
        0,
        "a revoked grant must stay revoked across restarts"
    );

    db.teardown().await;
}

#[tokio::test]
async fn does_not_overwrite_an_admin_edited_role_or_active_theme() {
    let db = TestDb::new("sys_seed_keeps_role_edits").await;

    db.conn
        .execute_unprepared(
            "UPDATE roles SET name = 'Staff', color = '#123456' WHERE slug = 'moderator'",
        )
        .await
        .expect("rename role");
    // An installed theme the admin switched to.
    db.conn
        .execute_unprepared(
            "INSERT INTO themes (id, slug, name, version, parent_slug, is_system, is_active) \
             VALUES (gen_random_uuid(), 'ferum-sumi', 'Ferum Sumi', '1.0.0', 'default', false, true); \
             UPDATE themes SET is_active = false WHERE slug = 'default'",
        )
        .await
        .expect("activate other theme");

    PgSystemSeedService::new(db.conn.clone())
        .seed_system()
        .await
        .expect("reseed");

    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM roles WHERE slug = 'moderator' AND name = 'Staff'"
        )
        .await,
        1,
        "an admin's role rename must survive a restart"
    );
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM themes WHERE slug = 'default' AND is_active"
        )
        .await,
        0,
        "the seeder must not reclaim is_active from the admin's chosen theme"
    );

    db.teardown().await;
}

#[tokio::test]
async fn a_recreated_system_role_comes_back_with_its_default_grants() {
    let db = TestDb::new("sys_seed_recreates_role").await;

    // A role that has gone missing must not return as an empty shell — a
    // moderator with zero permissions looks like a working install and is not.
    db.conn
        .execute_unprepared("DELETE FROM roles WHERE slug = 'moderator'")
        .await
        .expect("delete role");

    PgSystemSeedService::new(db.conn.clone())
        .seed_system()
        .await
        .expect("reseed");

    assert!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM role_permissions rp \
             JOIN roles r ON r.id = rp.role_id WHERE r.slug = 'moderator'"
        )
        .await
            > 10,
        "a recreated system role must get its whole default grant list back"
    );
    // And it must not have been handed admin's set by accident.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM role_permissions rp \
             JOIN roles r ON r.id = rp.role_id \
             JOIN permissions p ON p.id = rp.permission_id \
             WHERE r.slug = 'moderator' AND p.key LIKE 'admin.%'"
        )
        .await,
        0,
        "moderator must not receive admin permissions"
    );

    db.teardown().await;
}

#[tokio::test]
async fn repairs_a_missing_permission_row() {
    let db = TestDb::new("sys_seed_repairs_missing").await;

    // Simulates a database created before the key existed: the seeder must add
    // it AND grant it to the roles that hold it by default.
    db.conn
        .execute_unprepared("DELETE FROM permissions WHERE key = 'admin.languages'")
        .await
        .expect("drop permission");

    PgSystemSeedService::new(db.conn.clone())
        .seed_system()
        .await
        .expect("reseed");

    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM role_permissions rp \
             JOIN roles r ON r.id = rp.role_id \
             JOIN permissions p ON p.id = rp.permission_id \
             WHERE r.slug = 'admin' AND p.key = 'admin.languages'"
        )
        .await,
        1,
        "a newly created permission must reach the roles that grant it"
    );

    db.teardown().await;
}
