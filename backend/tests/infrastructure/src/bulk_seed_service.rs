//! Integration test for [`PgBulkSeedService`] — the opt-in example data.
//!
//! `#[ignore]` because a full run writes ~90,000 posts. Run it when the seeder
//! changes:
//!
//! ```text
//! cargo test -p ferum-infrastructure-tests bulk_seed -- --ignored --nocapture
//! ```
//!
//! Guards demo data that is misleading rather than absent: rows written in an
//! order that leaves a foreign key unfilled, counters asserted by hand.

use sea_orm::{DatabaseConnection, FromQueryResult, Statement};

use ferum_application::ports::BulkSeedService;
use ferum_infrastructure::bulk_seed_service::PgBulkSeedService;

use crate::common::{insert_user, TestDb};

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
#[ignore = "writes ~90k rows; run explicitly with --ignored"]
async fn seeds_a_coherent_example_forum() {
    let db = TestDb::new("bulk_seed_full").await;
    let admin = insert_user(&db.conn, 1).await;

    PgBulkSeedService::new(db.conn.clone())
        .seed_bulk(admin.id)
        .await
        .expect("seed_bulk");

    // The demo products must be filed under the catalogue taxonomy. They were
    // not, for as long as the filing lived in a migration that ran before any
    // product existed — which made the category filter demo as empty.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM products p \
             JOIN product_categories pc ON pc.id = p.category_id \
             WHERE p.slug = 'sofa-vang-boc-ni-scandinavian' AND pc.slug = 'sofa'"
        )
        .await,
        1,
        "the demo sofa must be filed under Sofa"
    );
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM products p \
             JOIN product_categories pc ON pc.id = p.category_id \
             WHERE p.slug = 'ban-an-go-oc-cho-6-ghe' AND pc.slug = 'ban'"
        )
        .await,
        1,
        "the demo dining table must be filed under Bàn"
    );
    // Left unfiled deliberately — a sheet of MDF is a material, not furniture —
    // so the admin's Unfiled count has something real in it.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM products \
             WHERE slug = 'van-mdf-phu-melamine' AND category_id IS NULL"
        )
        .await,
        1
    );

    // Every profile's post_count must equal the posts it actually has. This was
    // a hand-written `(i * 3) % 30`, contradicted by the posts the same run
    // inserted.
    assert_eq!(
        count(
            &db.conn,
            "SELECT count(*)::bigint AS n FROM users u \
             WHERE u.post_count <> ( \
                 SELECT count(*) FROM posts p \
                 WHERE p.author_id = u.id AND p.is_deleted = false \
             )"
        )
        .await,
        0,
        "no user's post_count may disagree with its posts"
    );

    // Surfaces that used to render empty on a seeded install.
    assert!(
        count(&db.conn, "SELECT count(*)::bigint AS n FROM reports WHERE status = 'pending'").await
            >= 2,
        "the mod queue needs pending reports to show"
    );
    assert!(
        count(&db.conn, "SELECT count(*)::bigint AS n FROM reports WHERE status <> 'pending'").await
            >= 2,
        "resolved and dismissed reports exercise the other queue tabs"
    );
    assert!(count(&db.conn, "SELECT count(*)::bigint AS n FROM bookmarks").await >= 4);
    assert!(count(&db.conn, "SELECT count(*)::bigint AS n FROM user_follows").await >= 4);

    // Re-running setup seeding must not duplicate anything: every insert is
    // ON CONFLICT DO NOTHING against a fixed id or a natural key.
    let before = count(&db.conn, "SELECT count(*)::bigint AS n FROM reports").await;
    PgBulkSeedService::new(db.conn.clone())
        .seed_bulk(admin.id)
        .await
        .expect("second seed_bulk");
    assert_eq!(
        count(&db.conn, "SELECT count(*)::bigint AS n FROM reports").await,
        before,
        "re-seeding must not duplicate demo reports"
    );

    db.teardown().await;
}
