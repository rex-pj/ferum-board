//! Integration tests for [`PgThreadRepository`].
//!
//! These tests verify the raw SQL fragments the compiler cannot check:
//!   - `LATERAL` join in `ENRICHED_SELECT` (first post excerpt + thumbnail)
//!   - `status::text` enum cast
//!   - All five `ThreadSort` modes in `list_by_category` and `list_feed`
//!   - `list_by_tag` JOIN with thread_tags + tags tables
//!   - `list_recent_by_categories` `ROW_NUMBER() OVER (PARTITION BY category_id)`
//!   - `list_admin_threads` `to_tsvector / plainto_tsquery` FTS filter
//!   - `try_record_view` `ON CONFLICT DO NOTHING` dedup logic

use ferum_domain::models::category::{PostPolicy, ViewPolicy};
use ferum_domain::repositories::category_repository::{CategoryRepository, NewCategory};
use ferum_domain::repositories::thread_repository::{
    AdminThreadFilter, NewThread, ThreadFilter, ThreadRepository, ThreadSort,
};
use ferum_domain::repositories::user_repository::{NewUser, UserRepository};
use ferum_infrastructure::repositories::{PgCategoryRepository, PgThreadRepository, PgUserRepository};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use uuid::Uuid;

use crate::common::TestDb;

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn make_user(db: &sea_orm::DatabaseConnection, n: u8) -> ferum_domain::models::user::User {
    PgUserRepository::new(db.clone())
        .create(NewUser {
            username: format!("user{n}"),
            email: format!("user{n}@example.com"),
            password_hash: Some("$2b$12$fakehash".to_string()),
        })
        .await
        .expect("create test user")
}

async fn make_category(
    db: &sea_orm::DatabaseConnection,
    slug: &str,
) -> ferum_domain::models::category::Category {
    PgCategoryRepository::new(db.clone())
        .create(NewCategory {
            slug: slug.to_string(),
            name: slug.to_uppercase(),
            description: None,
            parent_id: None,
            position: 0,
            view_policy: ViewPolicy::Public,
            post_policy: PostPolicy::Members,
            color: None,
            created_by_id: None,
        })
        .await
        .expect("create test category")
}

async fn make_thread(
    db: &sea_orm::DatabaseConnection,
    category_id: Uuid,
    author_id: Uuid,
    n: u8,
) -> ferum_domain::models::thread::Thread {
    PgThreadRepository::new(db.clone())
        .create(NewThread {
            id: Uuid::new_v4(),
            category_id,
            author_id,
            title: format!("Thread {n}"),
            slug: format!("thread-{n}"), product_id: None,
        })
        .await
        .expect("create test thread")
}

fn all_sort_modes() -> Vec<(&'static str, ThreadFilter)> {
    vec![
        ("Latest",     ThreadFilter { sort: ThreadSort::Latest     }),
        ("Newest",     ThreadFilter { sort: ThreadSort::Newest     }),
        ("Hottest",    ThreadFilter { sort: ThreadSort::Hottest    }),
        ("Unanswered", ThreadFilter { sort: ThreadSort::Unanswered }),
        ("Solved",     ThreadFilter { sort: ThreadSort::Solved     }),
    ]
}

// ─── Tests: read-only on empty schema ─────────────────────────────────────────

#[tokio::test]
async fn find_by_slug_lateral_join_executes_on_empty_schema() {
    let db = TestDb::new("thread_find_by_slug_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    // find_by_slug executes ENRICHED_SELECT which has:
    //   LEFT JOIN LATERAL (SELECT … FROM posts … LIMIT 1) p ON true
    let result = repo
        .find_by_slug("nonexistent-slug")
        .await
        .expect("find_by_slug LATERAL join executes on empty schema");
    assert!(result.is_none());

    db.teardown().await;
}

#[tokio::test]
async fn list_by_category_all_sort_modes_execute_on_empty_schema() {
    let db = TestDb::new("thread_list_by_cat_sorts_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());
    let cat_id = Uuid::new_v4();

    for (name, filter) in all_sort_modes() {
        let (threads, total) = repo
            .list_by_category(cat_id, &filter, 1, 20, None)
            .await
            .unwrap_or_else(|e| panic!("list_by_category sort={name} failed: {e:?}"));
        assert_eq!(total, 0, "sort={name}");
        assert!(threads.is_empty(), "sort={name}");
    }

    db.teardown().await;
}

#[tokio::test]
async fn list_feed_all_sort_modes_execute_on_empty_schema() {
    let db = TestDb::new("thread_list_feed_sorts_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());
    let cat_ids = [Uuid::new_v4(), Uuid::new_v4()];

    for (name, filter) in all_sort_modes() {
        let (threads, total) = repo
            .list_feed(&cat_ids, &filter, 1, 20, None)
            .await
            .unwrap_or_else(|e| panic!("list_feed sort={name} failed: {e:?}"));
        assert_eq!(total, 0, "sort={name}");
        assert!(threads.is_empty(), "sort={name}");
    }

    db.teardown().await;
}

#[tokio::test]
async fn list_by_tag_join_executes_on_empty_schema() {
    let db = TestDb::new("thread_list_by_tag_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    // list_by_tag does JOIN thread_tags + JOIN tags — both must resolve against
    // the migrated schema without SQL errors.
    let filter = ThreadFilter { sort: ThreadSort::Latest };
    let (threads, total) = repo
        .list_by_tag("nonexistent-tag", &[], &filter, 1, 20)
        .await
        .expect("list_by_tag JOIN executes on empty schema");
    assert_eq!(total, 0);
    assert!(threads.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn list_recent_by_categories_window_function_executes_on_empty_schema() {
    let db = TestDb::new("thread_list_recent_window_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());
    let cat_ids = [Uuid::new_v4(), Uuid::new_v4()];

    // ROW_NUMBER() OVER (PARTITION BY category_id ORDER BY …)
    let threads = repo
        .list_recent_by_categories(&cat_ids, 5)
        .await
        .expect("ROW_NUMBER() OVER (PARTITION BY) window function executes");
    assert!(threads.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn list_admin_threads_fts_filter_executes_on_empty_schema() {
    let db = TestDb::new("thread_list_admin_fts_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    // to_tsvector('simple', t.title) @@ plainto_tsquery('simple', $1)
    let filter = AdminThreadFilter {
        search: Some("rust programming".to_string()),
        ..Default::default()
    };
    let (threads, total) = repo
        .list_admin_threads(&filter, 1, 20)
        .await
        .expect("to_tsvector/plainto_tsquery FTS filter executes on empty schema");
    assert_eq!(total, 0);
    assert!(threads.is_empty());

    db.teardown().await;
}

// ─── Tests: create → query roundtrip ──────────────────────────────────────────

#[tokio::test]
async fn create_and_find_by_slug_roundtrip() {
    let db = TestDb::new("thread_create_find_slug").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    // find_by_slug uses ENRICHED_SELECT with LATERAL — exercises the full query
    let found = repo
        .find_by_slug("thread-1")
        .await
        .expect("find_by_slug executes")
        .expect("thread should exist");

    assert_eq!(found.id, thread.id);
    assert_eq!(found.title, "Thread 1");
    assert_eq!(found.author_username.as_deref(), Some("user1"));

    db.teardown().await;
}

#[tokio::test]
async fn list_by_category_returns_created_threads() {
    let db = TestDb::new("thread_list_by_cat_data").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "tech").await;
    make_thread(&db.conn, cat.id, user.id, 1).await;
    make_thread(&db.conn, cat.id, user.id, 2).await;

    let filter = ThreadFilter { sort: ThreadSort::Latest };
    let (threads, total) = repo
        .list_by_category(cat.id, &filter, 1, 20, None)
        .await
        .expect("list_by_category executes with data");

    assert_eq!(total, 2);
    assert_eq!(threads.len(), 2);

    db.teardown().await;
}

#[tokio::test]
async fn list_by_category_unanswered_sort_filters_correctly() {
    let db = TestDb::new("thread_unanswered_sort").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "help").await;
    make_thread(&db.conn, cat.id, user.id, 1).await; // reply_count = 0 → unanswered

    // Exercises: WHERE t.reply_count = 0 AND t.status = 'open' (enum literal cast)
    let filter = ThreadFilter { sort: ThreadSort::Unanswered };
    let (threads, total) = repo
        .list_by_category(cat.id, &filter, 1, 20, None)
        .await
        .expect("Unanswered sort with status enum cast executes");

    assert_eq!(total, 1);
    assert_eq!(threads.len(), 1);

    db.teardown().await;
}

#[tokio::test]
async fn list_recent_by_categories_partitions_per_category() {
    let db = TestDb::new("thread_recent_partition").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat_a = make_category(&db.conn, "cat-a").await;
    let cat_b = make_category(&db.conn, "cat-b").await;

    for i in 1u8..=3 {
        make_thread(&db.conn, cat_a.id, user.id, i).await;
    }
    make_thread(&db.conn, cat_b.id, user.id, 10).await;
    make_thread(&db.conn, cat_b.id, user.id, 11).await;

    // limit_per_category=2: cat_a gives 2, cat_b gives 2 → total 4
    let threads = repo
        .list_recent_by_categories(&[cat_a.id, cat_b.id], 2)
        .await
        .expect("ROW_NUMBER() OVER (PARTITION BY) executes with data");

    assert_eq!(threads.len(), 4, "2 from cat_a + 2 from cat_b");

    db.teardown().await;
}

#[tokio::test]
async fn try_record_view_on_conflict_dedup_logic() {
    let db = TestDb::new("thread_record_view_dedup").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "views").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    let viewer_key = "user:abc123";

    let first = repo
        .try_record_view(thread.id, viewer_key, "user")
        .await
        .expect("try_record_view first call (INSERT)");
    assert!(first, "first view of the day should be counted");

    // ON CONFLICT DO NOTHING → same-day check → no new view
    let second = repo
        .try_record_view(thread.id, viewer_key, "user")
        .await
        .expect("try_record_view second call (ON CONFLICT)");
    assert!(!second, "same viewer on same day must not count as a new view");

    db.teardown().await;
}

// ─── list_latest_reviews ──────────────────────────────────────────────────────
// This query is hand-written SQL (DISTINCT ON subquery + the published-product
// guard), so the type checker sees none of its logic. The tests below pin the
// three things that logic exists for: one row per product, newest-first, drafts
// excluded.

/// Insert a minimal published product. Raw SQL rather than the product repository:
/// the FK on `threads.product_id` needs a real row, and these tests only care that
/// the row exists and carries a `status`, not about the full product model.
async fn make_product(db: &sea_orm::DatabaseConnection, slug: &str, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO products (id, slug, name, product_type, status, currency, dimensions, created_at, updated_at) \
         VALUES ($1, $2, $2, 'furniture', $3::product_status, 'VND', '{}'::jsonb, now(), now())",
        [id.into(), slug.into(), status.into()],
    ))
    .await
    .expect("insert product");
    id
}

/// Create a review thread (thread linked to a product) and stamp its `created_at`
/// to a fixed instant, so newest-per-product and overall ordering are deterministic
/// rather than at the mercy of insert timing within one clock tick.
async fn make_review(
    db: &sea_orm::DatabaseConnection,
    category_id: Uuid,
    author_id: Uuid,
    product_id: Uuid,
    n: u8,
    created_at: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    PgThreadRepository::new(db.clone())
        .create(NewThread {
            id,
            category_id,
            author_id,
            title: format!("Review {n}"),
            slug: format!("review-{n}"),
            product_id: Some(product_id),
        })
        .await
        .expect("create review thread");
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE threads SET created_at = $2::timestamptz WHERE id = $1",
        [id.into(), created_at.into()],
    ))
    .await
    .expect("stamp created_at");
    id
}

#[tokio::test]
async fn list_latest_reviews_collapses_to_one_row_per_product() {
    let db = TestDb::new("thread_latest_reviews_collapse").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let cat = make_category(&db.conn, "reviews").await;
    let u1 = make_user(&db.conn, 1).await;
    let u2 = make_user(&db.conn, 2).await;
    let sofa = make_product(&db.conn, "sofa", "published").await;
    let chair = make_product(&db.conn, "chair", "published").await;

    // Sofa reviewed by two different people (allowed — the per-author unique index
    // only stops one account reviewing the same product twice). Chair once.
    make_review(&db.conn, cat.id, u1.id, sofa, 1, "2026-01-01T10:00:00Z").await;
    let sofa_newer =
        make_review(&db.conn, cat.id, u2.id, sofa, 2, "2026-01-03T10:00:00Z").await;
    let chair_only =
        make_review(&db.conn, cat.id, u1.id, chair, 3, "2026-01-02T10:00:00Z").await;

    let got = repo.list_latest_reviews(10).await.expect("list_latest_reviews");
    let ids: Vec<Uuid> = got.iter().map(|t| t.id).collect();

    // One row per product: the sofa contributes only its newer review, not both.
    // Overall order is newest-first, so sofa (Jan 3) precedes chair (Jan 2).
    assert_eq!(
        ids,
        vec![sofa_newer, chair_only],
        "one row per product, newest per product, newest-first overall"
    );
    db.teardown().await;
}

#[tokio::test]
async fn list_latest_reviews_excludes_draft_products() {
    let db = TestDb::new("thread_latest_reviews_draft").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let cat = make_category(&db.conn, "reviews").await;
    let u1 = make_user(&db.conn, 1).await;
    let live = make_product(&db.conn, "live", "published").await;
    let pending = make_product(&db.conn, "pending", "draft").await;

    let live_review =
        make_review(&db.conn, cat.id, u1.id, live, 1, "2026-01-01T10:00:00Z").await;
    // Newer, but its product is still a draft — must not surface publicly.
    make_review(&db.conn, cat.id, u1.id, pending, 2, "2026-01-05T10:00:00Z").await;

    let got = repo.list_latest_reviews(10).await.expect("list_latest_reviews");
    let ids: Vec<Uuid> = got.iter().map(|t| t.id).collect();

    assert_eq!(
        ids,
        vec![live_review],
        "a review of a draft product is hidden even though it is the newest"
    );
    db.teardown().await;
}

#[tokio::test]
async fn list_latest_reviews_empty_when_no_reviews() {
    let db = TestDb::new("thread_latest_reviews_empty").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    // A plain discussion thread (no product_id) must never appear here.
    let cat = make_category(&db.conn, "general").await;
    let u1 = make_user(&db.conn, 1).await;
    make_thread(&db.conn, cat.id, u1.id, 1).await;

    let got = repo.list_latest_reviews(10).await.expect("list_latest_reviews");
    assert!(got.is_empty(), "non-review threads are not reviews");
    db.teardown().await;
}

#[tokio::test]
async fn update_reply_stats_increments_count() {
    let db = TestDb::new("thread_update_reply_stats").await;
    let repo = PgThreadRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "replies").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    repo.update_reply_stats(thread.id, 3, Some(chrono::Utc::now()))
        .await
        .expect("update_reply_stats executes");

    let found = repo
        .find_by_id(thread.id)
        .await
        .expect("find_by_id")
        .expect("thread should exist");
    assert_eq!(found.reply_count, 3);

    db.teardown().await;
}
