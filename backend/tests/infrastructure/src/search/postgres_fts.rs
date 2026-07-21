//! Integration tests for [`PostgresFtsService`].
//!
//! These tests exercise the raw SQL that uses `to_tsvector`, `to_tsquery`,
//! `ts_rank`, `ts_headline`, `COUNT(*)::BIGINT`, and the `thread_status` enum cast.
//! The FTS trigger (`trg_thread_search`) populates `search_vector` automatically.

use ferum_application::ports::{SearchQuery, SearchService};
use ferum_infrastructure::search::PostgresFtsService;

use crate::common::{insert_category, insert_thread, insert_user, TestDb};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn query(q: &str, category_id: Option<uuid::Uuid>, page: u64, per_page: u64) -> SearchQuery {
    SearchQuery { q: q.to_string(), category_ids: category_id.into_iter().collect(), page, per_page }
}

// ─── Basic sanity ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_empty_query_returns_empty_without_hitting_db() {
    let db = TestDb::new("fts_empty_query").await;
    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc.search(query("", None, 1, 20)).await.expect("search");
    assert_eq!(result.total, 0);
    assert!(result.hits.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn search_on_empty_db_returns_zero() {
    // The FTS SQL (to_tsvector / to_tsquery / ts_rank / COUNT::BIGINT) must parse correctly
    // against the live schema even when no threads exist.
    let db = TestDb::new("fts_no_threads").await;
    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc.search(query("rust", None, 1, 20)).await.expect("search on empty");
    assert_eq!(result.total, 0);
    assert!(result.hits.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn search_finds_thread_by_title_word() {
    let db = TestDb::new("fts_find_by_title").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    insert_thread(&db.conn, 1, cat.id, user.id).await; // "Thread 1"

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc.search(query("Thread", None, 1, 20)).await.expect("search");
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].title, "Thread 1");
    db.teardown().await;
}

#[tokio::test]
async fn search_prefix_match_finds_partial_word() {
    // tsquery uses "word:*" — prefix match must work.
    let db = TestDb::new("fts_prefix_match").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    // Create thread with a distinctive title
    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    PgThreadRepository::new(db.conn.clone())
        .create(NewThread {
            id: uuid::Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: "Introduction to Rustaceans".to_string(),
            slug: "intro-rustaceans".to_string(), product_id: None,
        })
        .await.expect("create thread");

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc.search(query("Rust", None, 1, 20)).await.expect("search");
    assert_eq!(result.total, 1, "prefix 'Rust' should match 'Rustaceans'");
    db.teardown().await;
}

#[tokio::test]
async fn search_with_category_filter_excludes_other_categories() {
    let db = TestDb::new("fts_category_filter").await;
    let user = insert_user(&db.conn, 1).await;
    let cat_a = insert_category(&db.conn, 1).await;
    let cat_b = insert_category(&db.conn, 2).await;

    // Both threads have the same word in title but belong to different categories
    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    let repo = PgThreadRepository::new(db.conn.clone());
    repo.create(NewThread { id: uuid::Uuid::new_v4(), category_id: cat_a.id, author_id: user.id, title: "Rust basics".to_string(), slug: "rust-basics".to_string(), product_id: None }).await.expect("cat_a thread");
    repo.create(NewThread { id: uuid::Uuid::new_v4(), category_id: cat_b.id, author_id: user.id, title: "Rust advanced".to_string(), slug: "rust-advanced".to_string(), product_id: None }).await.expect("cat_b thread");

    let svc = PostgresFtsService::new(db.conn.clone());

    // No filter → both found
    let all = svc.search(query("Rust", None, 1, 20)).await.expect("all");
    assert_eq!(all.total, 2);

    // Filter to cat_a only → 1 result
    let filtered = svc.search(query("Rust", Some(cat_a.id), 1, 20)).await.expect("filtered");
    assert_eq!(filtered.total, 1);
    db.teardown().await;
}

#[tokio::test]
async fn search_pagination_works() {
    let db = TestDb::new("fts_pagination").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    let repo = PgThreadRepository::new(db.conn.clone());
    for i in 1..=5u8 {
        repo.create(NewThread {
            id: uuid::Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: format!("Forum topic number {i}"),
            slug: format!("topic-{i}"), product_id: None,
        }).await.expect("create");
    }

    let svc = PostgresFtsService::new(db.conn.clone());
    let page1 = svc.search(query("topic", None, 1, 3)).await.expect("page 1");
    assert_eq!(page1.total, 5);
    assert_eq!(page1.hits.len(), 3);

    let page2 = svc.search(query("topic", None, 2, 3)).await.expect("page 2");
    assert_eq!(page2.hits.len(), 2);
    db.teardown().await;
}

#[tokio::test]
async fn search_returns_excerpt_from_ts_headline() {
    let db = TestDb::new("fts_excerpt").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    PgThreadRepository::new(db.conn.clone())
        .create(NewThread {
            id: uuid::Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: "Deep dive into async Rust patterns".to_string(),
            slug: "async-rust-patterns".to_string(), product_id: None,
        })
        .await.expect("create");

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc.search(query("async", None, 1, 20)).await.expect("search");
    assert_eq!(result.total, 1);
    // ts_headline should produce an excerpt; it may be None only if the engine returns NULL
    // (which shouldn't happen for a matching result)
    assert!(result.hits[0].excerpt.is_some(), "ts_headline should produce an excerpt");
    db.teardown().await;
}
