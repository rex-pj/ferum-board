//! Integration tests for [`PostgresFtsService`].
//!
//! These tests exercise the raw SQL that uses `to_tsvector`, `to_tsquery`,
//! `ts_rank`, `ts_headline`, `COUNT(*)::BIGINT`, the `thread_status` enum cast,
//! and the `products.search_vector` generated column installed by migration 31.

use ferum_application::ports::{
    ProductFacets, ProductSearchSort, SearchKind, SearchQuery, SearchService, ThreadSearchSort,
};
use ferum_domain::models::brand::{NewBrand, UpdateBrand};
use ferum_domain::models::product::{NewProduct, ProductStatus, ProductType};
use ferum_domain::repositories::brand_repository::BrandRepository;
use ferum_domain::repositories::product_repository::{ProductRepository, UpdateProduct};
use ferum_infrastructure::repositories::{PgBrandRepository, PgProductRepository};
use ferum_infrastructure::search::PostgresFtsService;
use uuid::Uuid;

use crate::common::{insert_category, insert_thread, insert_user, TestDb};

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// `categories` is the set of categories the viewer may see — the port treats an
/// empty set as "no thread hits", so every thread test names its categories.
fn thread_query(q: &str, categories: Vec<Uuid>, page: u64, per_page: u64) -> SearchQuery {
    thread_query_sorted(q, categories, ThreadSearchSort::default(), page, per_page)
}

fn thread_query_sorted(
    q: &str,
    categories: Vec<Uuid>,
    thread_sort: ThreadSearchSort,
    page: u64,
    per_page: u64,
) -> SearchQuery {
    SearchQuery {
        q: q.to_string(),
        kind: SearchKind::Thread,
        facets: ProductFacets::default(),
        thread_sort,
        visible_category_ids: categories,
        in_category_ids: vec![],
        viewer_id: None,
        page,
        per_page,
        count_only: false,
    }
}

fn product_query(q: &str, viewer_id: Option<Uuid>, page: u64, per_page: u64) -> SearchQuery {
    SearchQuery {
        q: q.to_string(),
        kind: SearchKind::Product,
        facets: ProductFacets::default(),
        thread_sort: ThreadSearchSort::default(),
        visible_category_ids: vec![],
        in_category_ids: vec![],
        viewer_id,
        page,
        per_page,
        count_only: false,
    }
}

async fn insert_product(
    conn: &sea_orm::DatabaseConnection,
    name: &str,
    slug: &str,
    status: ProductStatus,
    created_by: Option<Uuid>,
) -> Uuid {
    let repo = PgProductRepository::new(conn.clone());
    let p = repo
        .create(NewProduct {
            id: Uuid::new_v4(),
            slug: slug.to_string(),
            name: name.to_string(),
            product_type: ProductType::Furniture,
            brand_id: None,
            category_id: None,
            style: None,
            price_min: None,
            price_max: None,
            currency: "VND".to_string(),
            dimensions: serde_json::json!({}),
            origin: None,
            description_md: None,
            created_by_id: created_by,
            material_ids: vec![],
        })
        .await
        .expect("create product");

    if status != ProductStatus::Draft {
        repo.update(
            p.id,
            UpdateProduct {
                status: Some(status),
                ..Default::default()
            },
        )
        .await
        .expect("publish product");
    }
    p.id
}

// ─── Basic sanity ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_empty_query_returns_empty_without_hitting_db() {
    let db = TestDb::new("fts_empty_query").await;
    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("", vec![Uuid::new_v4()], 1, 20))
        .await
        .expect("search");
    assert_eq!(result.total, 0);
    assert!(result.hits.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn search_on_empty_db_returns_zero() {
    // The FTS SQL (to_tsvector / f_unaccent / ts_rank / COUNT::BIGINT) must
    // parse correctly against the live schema even when no threads exist.
    let db = TestDb::new("fts_no_threads").await;
    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("rust", vec![Uuid::new_v4()], 1, 20))
        .await
        .expect("search on empty");
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
    let result = svc
        .search(thread_query("Thread", vec![cat.id], 1, 20))
        .await
        .expect("search");
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].title, "Thread 1");
    assert_eq!(result.hits[0].kind, SearchKind::Thread);
    db.teardown().await;
}

/// `insert_thread` fixes reply_count (0) and created_at (now), so a sort test
/// sets them directly.
async fn set_thread_metrics(
    conn: &sea_orm::DatabaseConnection,
    id: Uuid,
    reply_count: i32,
    created_at: &str,
) {
    use sea_orm::ConnectionTrait;
    conn.execute_raw(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        format!(
            "UPDATE threads SET reply_count = {reply_count}, created_at = '{created_at}' \
             WHERE id = '{id}'"
        ),
    ))
    .await
    .expect("set thread metrics");
}

/// Discussion search honours the requested order. Relevance is equal across the
/// three (each title carries "Thread" once), so newest and most-replies are the
/// orderings under test.
#[tokio::test]
async fn thread_search_orders_by_the_requested_sort() {
    let db = TestDb::new("fts_thread_sort").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let a = insert_thread(&db.conn, 1, cat.id, user.id).await; // "Thread 1"
    let b = insert_thread(&db.conn, 2, cat.id, user.id).await; // "Thread 2"
    let c = insert_thread(&db.conn, 3, cat.id, user.id).await; // "Thread 3"

    // A: 5 replies, oldest.  B: 1 reply, newest.  C: 10 replies, middle.
    set_thread_metrics(&db.conn, a.id, 5, "2026-01-01T00:00:00Z").await;
    set_thread_metrics(&db.conn, b.id, 1, "2026-03-01T00:00:00Z").await;
    set_thread_metrics(&db.conn, c.id, 10, "2026-02-01T00:00:00Z").await;

    let svc = PostgresFtsService::new(db.conn.clone());

    let by_replies = svc
        .search(thread_query_sorted(
            "Thread",
            vec![cat.id],
            ThreadSearchSort::MostReplies,
            1,
            20,
        ))
        .await
        .expect("search by replies");
    let titles: Vec<&str> = by_replies.hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Thread 3", "Thread 1", "Thread 2"],
        "most replies first"
    );

    let by_new = svc
        .search(thread_query_sorted(
            "Thread",
            vec![cat.id],
            ThreadSearchSort::Newest,
            1,
            20,
        ))
        .await
        .expect("search by newest");
    let titles: Vec<&str> = by_new.hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Thread 2", "Thread 3", "Thread 1"],
        "newest first"
    );

    db.teardown().await;
}

/// The security property, stated as a test: with no visible categories there
/// are no thread hits, however well the query matches. A caller that fails to
/// resolve visibility must get silence, not every `staff_only` title on the
/// forum.
#[tokio::test]
async fn search_with_no_visible_categories_returns_nothing() {
    let db = TestDb::new("fts_no_visible_cats").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    insert_thread(&db.conn, 1, cat.id, user.id).await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("Thread", vec![], 1, 20))
        .await
        .expect("search");
    assert_eq!(result.total, 0);
    assert!(result.hits.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn search_prefix_match_finds_partial_word() {
    // tsquery uses "word:*" — prefix match must work.
    let db = TestDb::new("fts_prefix_match").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    PgThreadRepository::new(db.conn.clone())
        .create(NewThread {
            id: uuid::Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: "Introduction to Rustaceans".to_string(),
            slug: "intro-rustaceans".to_string(),
            product_id: None,
        })
        .await
        .expect("create thread");

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("Rust", vec![cat.id], 1, 20))
        .await
        .expect("search");
    assert_eq!(result.total, 1, "prefix 'Rust' should match 'Rustaceans'");
    db.teardown().await;
}

/// tsquery operators in user input must be inert. Without stripping, `a & b`
/// reaches `to_tsquery` as an expression and either errors or changes the
/// query's shape.
#[tokio::test]
async fn search_ignores_tsquery_operator_characters() {
    let db = TestDb::new("fts_operator_chars").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    insert_thread(&db.conn, 1, cat.id, user.id).await; // "Thread 1"

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("Thread & | ! ( )", vec![cat.id], 1, 20))
        .await
        .expect("operator characters must not break the query");
    assert_eq!(result.total, 1);
    db.teardown().await;
}

#[tokio::test]
async fn search_with_category_filter_excludes_other_categories() {
    let db = TestDb::new("fts_category_filter").await;
    let user = insert_user(&db.conn, 1).await;
    let cat_a = insert_category(&db.conn, 1).await;
    let cat_b = insert_category(&db.conn, 2).await;

    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    let repo = PgThreadRepository::new(db.conn.clone());
    repo.create(NewThread { id: uuid::Uuid::new_v4(), category_id: cat_a.id, author_id: user.id, title: "Rust basics".to_string(), slug: "rust-basics".to_string(), product_id: None }).await.expect("cat_a thread");
    repo.create(NewThread { id: uuid::Uuid::new_v4(), category_id: cat_b.id, author_id: user.id, title: "Rust advanced".to_string(), slug: "rust-advanced".to_string(), product_id: None }).await.expect("cat_b thread");

    let svc = PostgresFtsService::new(db.conn.clone());

    let all = svc
        .search(thread_query("Rust", vec![cat_a.id, cat_b.id], 1, 20))
        .await
        .expect("all");
    assert_eq!(all.total, 2);

    let filtered = svc
        .search(thread_query("Rust", vec![cat_a.id], 1, 20))
        .await
        .expect("filtered");
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
            slug: format!("topic-{i}"),
            product_id: None,
        })
        .await
        .expect("create");
    }

    let svc = PostgresFtsService::new(db.conn.clone());
    let page1 = svc
        .search(thread_query("topic", vec![cat.id], 1, 3))
        .await
        .expect("page 1");
    assert_eq!(page1.total, 5);
    assert_eq!(page1.hits.len(), 3);

    let page2 = svc
        .search(thread_query("topic", vec![cat.id], 2, 3))
        .await
        .expect("page 2");
    assert_eq!(page2.hits.len(), 2);
    db.teardown().await;
}

#[tokio::test]
async fn search_count_only_returns_total_without_hits() {
    let db = TestDb::new("fts_count_only").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    insert_thread(&db.conn, 1, cat.id, user.id).await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let mut q = thread_query("Thread", vec![cat.id], 1, 20);
    q.count_only = true;
    let result = svc.search(q).await.expect("count only");
    assert_eq!(result.total, 1);
    assert!(result.hits.is_empty(), "count_only must not fetch rows");
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
            slug: "async-rust-patterns".to_string(),
            product_id: None,
        })
        .await
        .expect("create");

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("async", vec![cat.id], 1, 20))
        .await
        .expect("search");
    assert_eq!(result.total, 1);
    assert!(
        result.hits[0].excerpt.is_some(),
        "ts_headline should produce an excerpt"
    );
    db.teardown().await;
}

// ─── Post content ─────────────────────────────────────────────────────────────
//
// F-ORG-03 is "thread title + post content". `idx_posts_fts` was built in
// migration 6 and then queried by nothing, so for the whole life of the feature
// a term that appeared only in a reply was unfindable — which on a forum is most
// of the text there is. These cover the half that was missing, and the
// visibility rules it has to respect.

async fn insert_post_with(
    conn: &sea_orm::DatabaseConnection,
    thread_id: Uuid,
    author_id: Uuid,
    content_md: &str,
    status: ferum_domain::models::post::PostStatus,
) -> Uuid {
    use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
    use ferum_infrastructure::repositories::PgPostRepository;
    PgPostRepository::new(conn.clone())
        .create(NewPost {
            thread_id,
            author_id,
            parent_id: None,
            content_md: content_md.to_string(),
            content_html: format!("<p>{content_md}</p>"),
            status,
        })
        .await
        .expect("create post")
        .id
}

#[tokio::test]
async fn search_finds_a_thread_by_a_word_only_its_body_contains() {
    let db = TestDb::new("fts_post_body").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await; // title "Thread 1"

    // "carburettor" appears nowhere in the title, so a title-only search cannot
    // find this thread however well the reply matches.
    insert_post_with(
        &db.conn,
        thread.id,
        user.id,
        "You will want to clean the carburettor before reassembling anything.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("carburettor", vec![cat.id], 1, 20))
        .await
        .expect("search");

    assert_eq!(result.total, 1, "a term found only in a reply must still match");
    assert_eq!(result.hits[0].id, thread.id);
    assert_eq!(
        result.hits[0].kind,
        SearchKind::Thread,
        "a body match is still a thread hit — posts are not a separate result kind"
    );
    db.teardown().await;
}

/// The excerpt is the reason a body hit is legible: without it the reader sees a
/// title that does not contain their term and no clue why it was returned.
#[tokio::test]
async fn a_body_match_excerpts_the_post_not_the_title() {
    let db = TestDb::new("fts_post_body_excerpt").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    insert_post_with(
        &db.conn,
        thread.id,
        user.id,
        "The correct torque for that flywheel bolt is ninety newton metres.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("flywheel", vec![cat.id], 1, 20))
        .await
        .expect("search");

    let excerpt = result.hits[0].excerpt.as_deref().expect("body hit needs an excerpt");
    assert!(
        excerpt.contains("flywheel"),
        "excerpt should come from the matching post, got {excerpt:?}"
    );
    assert!(
        excerpt.contains("<b>"),
        "ts_headline should mark the term, and <b> must survive sanitisation: {excerpt:?}"
    );
    db.teardown().await;
}

/// Diacritic folding has to reach the body too. Folding only the title would
/// leave Vietnamese replies — the bulk of the prose on this forum — unsearchable
/// for anyone who types tone marks, which is most people.
#[tokio::test]
async fn a_body_match_folds_diacritics_in_both_directions() {
    let db = TestDb::new("fts_post_body_unaccent").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    insert_post_with(
        &db.conn,
        thread.id,
        user.id,
        "Mình dùng đệm lò xo túi độc lập, nằm khá êm.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    use sea_orm::{ConnectionTrait, Statement};
    let unaccent_active: bool = db
        .conn
        .query_one_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT (f_unaccent('ế') = 'e') AS folded".to_owned(),
        ))
        .await
        .expect("probe f_unaccent")
        .and_then(|r| r.try_get::<bool>("", "folded").ok())
        .unwrap_or(false);
    if !unaccent_active {
        eprintln!("unaccent extension unavailable — skipping diacritic-folding assertion");
        db.teardown().await;
        return;
    }

    let svc = PostgresFtsService::new(db.conn.clone());
    for q in ["đệm", "dem", "độc lập", "doc lap"] {
        let result = svc
            .search(thread_query(q, vec![cat.id], 1, 20))
            .await
            .expect("search");
        assert_eq!(result.total, 1, "query {q:?} should reach the post body");
    }
    db.teardown().await;
}

/// A soft-deleted post renders as a tombstone in the thread; its text must not
/// keep pulling the thread into results. A pending post is awaiting moderation
/// and must not be reachable at all — search is where unreviewed text would
/// otherwise get its first audience.
#[tokio::test]
async fn deleted_and_pending_post_bodies_are_not_searchable() {
    let db = TestDb::new("fts_post_body_hidden").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    let deleted_thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post_with(
        &db.conn,
        deleted_thread.id,
        user.id,
        "A retracted claim about magnetrons.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    let pending_thread = insert_thread(&db.conn, 2, cat.id, user.id).await;
    insert_post_with(
        &db.conn,
        pending_thread.id,
        user.id,
        "An unreviewed claim about magnetrons.",
        ferum_domain::models::post::PostStatus::Pending,
    )
    .await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let cats = vec![cat.id];

    // Before deletion the published post is findable — otherwise the assertion
    // after it would pass for the wrong reason.
    assert_eq!(
        svc.search(thread_query("magnetrons", cats.clone(), 1, 20)).await.unwrap().total,
        1,
        "only the published post should match; the pending one must already be excluded"
    );

    use ferum_domain::repositories::post_repository::PostRepository;
    ferum_infrastructure::repositories::PgPostRepository::new(db.conn.clone())
        .soft_delete(post, user.id)
        .await
        .expect("soft delete");

    assert_eq!(
        svc.search(thread_query("magnetrons", cats, 1, 20)).await.unwrap().total,
        0,
        "neither a deleted nor a pending body may match"
    );
    db.teardown().await;
}

/// A thread is one hit however many of its posts match — the result set is
/// threads, so N matching replies must not produce N rows of the same thread.
#[tokio::test]
async fn many_matching_posts_still_yield_one_thread_hit() {
    let db = TestDb::new("fts_post_body_dedup").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    for i in 1..=4 {
        insert_post_with(
            &db.conn,
            thread.id,
            user.id,
            &format!("Reply {i} also mentions the alternator."),
            ferum_domain::models::post::PostStatus::Published,
        )
        .await;
    }

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc
        .search(thread_query("alternator", vec![cat.id], 1, 20))
        .await
        .expect("search");

    assert_eq!(result.total, 1, "count must not multiply by matching posts");
    assert_eq!(result.hits.len(), 1);
    db.teardown().await;
}

/// A body match must not smuggle a thread past the category filter — the
/// visibility set is the security boundary and the new OR branch sits inside it.
#[tokio::test]
async fn a_body_match_still_obeys_category_visibility() {
    let db = TestDb::new("fts_post_body_visibility").await;
    let user = insert_user(&db.conn, 1).await;
    let visible = insert_category(&db.conn, 1).await;
    let hidden = insert_category(&db.conn, 2).await;
    let thread = insert_thread(&db.conn, 1, hidden.id, user.id).await;

    insert_post_with(
        &db.conn,
        thread.id,
        user.id,
        "Staff-only discussion of the crankshaft incident.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    let svc = PostgresFtsService::new(db.conn.clone());
    assert_eq!(
        svc.search(thread_query("crankshaft", vec![visible.id], 1, 20)).await.unwrap().total,
        0,
        "a post in an invisible category must not surface its thread"
    );
    assert_eq!(
        svc.search(thread_query("crankshaft", vec![visible.id, hidden.id], 1, 20))
            .await
            .unwrap()
            .total,
        1,
        "and must surface once the category is visible"
    );
    db.teardown().await;
}

/// Title beats body. A thread named for the query must outrank one that merely
/// mentions it in passing, or searching for a topic returns whichever thread
/// happens to be chattiest about it.
#[tokio::test]
async fn a_title_match_outranks_a_body_only_match() {
    let db = TestDb::new("fts_post_body_rank").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    let repo = PgThreadRepository::new(db.conn.clone());
    let by_title = repo
        .create(NewThread {
            id: Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: "Choosing a differential".to_string(),
            slug: "choosing-differential".to_string(),
            product_id: None,
        })
        .await
        .expect("title thread");
    let by_body = repo
        .create(NewThread {
            id: Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: "Weekend garage log".to_string(),
            slug: "weekend-garage-log".to_string(),
            product_id: None,
        })
        .await
        .expect("body thread");

    insert_post_with(
        &db.conn,
        by_body.id,
        user.id,
        "Spent Sunday staring at the differential and gave up.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let hits = svc
        .search(thread_query_sorted(
            "differential",
            vec![cat.id],
            ThreadSearchSort::Relevance,
            1,
            20,
        ))
        .await
        .expect("search");

    assert_eq!(hits.total, 2, "both routes to a match must return a hit");
    assert_eq!(
        hits.hits[0].id, by_title.id,
        "a title hit must outrank a body-only hit"
    );
    assert_eq!(hits.hits[1].id, by_body.id);
    db.teardown().await;
}

/// The page is chosen in a subquery and the body snippet joined on afterwards; a
/// subquery's ORDER BY is not carried through a join by any rule Postgres
/// guarantees, so the outer ORDER BY is load-bearing. This walks every sort with
/// body matches present, where a dropped ordering shows up as a scrambled page.
#[tokio::test]
async fn every_thread_sort_survives_the_body_snippet_join() {
    let db = TestDb::new("fts_post_body_sorts").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let a = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let b = insert_thread(&db.conn, 2, cat.id, user.id).await;
    let c = insert_thread(&db.conn, 3, cat.id, user.id).await;

    // Every thread matches only through its body, so relevance is equal and the
    // explicit orderings are what is under test.
    for t in [&a, &b, &c] {
        insert_post_with(
            &db.conn,
            t.id,
            user.id,
            "Notes on the camshaft profile.",
            ferum_domain::models::post::PostStatus::Published,
        )
        .await;
    }
    set_thread_metrics(&db.conn, a.id, 5, "2026-01-01T00:00:00Z").await;
    set_thread_metrics(&db.conn, b.id, 1, "2026-03-01T00:00:00Z").await;
    set_thread_metrics(&db.conn, c.id, 10, "2026-02-01T00:00:00Z").await;

    let svc = PostgresFtsService::new(db.conn.clone());

    let by_new = svc
        .search(thread_query_sorted("camshaft", vec![cat.id], ThreadSearchSort::Newest, 1, 20))
        .await
        .expect("newest");
    assert_eq!(
        by_new.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
        vec![b.id, c.id, a.id],
        "newest first"
    );

    let by_replies = svc
        .search(thread_query_sorted("camshaft", vec![cat.id], ThreadSearchSort::MostReplies, 1, 20))
        .await
        .expect("most replies");
    assert_eq!(
        by_replies.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
        vec![c.id, a.id, b.id],
        "most replies first"
    );
    db.teardown().await;
}

/// `idx_posts_fts` existed for the whole life of this feature and was used by
/// nothing. Now that something queries it, the way that regresses is no longer
/// "no results" but "a sequential scan of every post on every search" — silent,
/// correct, and ruinous. `enable_seqscan = off` makes the planner state its
/// preference: if the predicate matches the index it reaches for it, and if the
/// expression has drifted it falls back to a Seq Scan even under the penalty.
#[tokio::test]
async fn the_body_predicate_actually_uses_idx_posts_fts() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};

    let db = TestDb::new("fts_post_body_index").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    insert_post_with(
        &db.conn,
        thread.id,
        user.id,
        "Notes on the camshaft profile.",
        ferum_domain::models::post::PostStatus::Published,
    )
    .await;

    let txn = db.conn.begin().await.expect("begin");
    // SET LOCAL, so the setting cannot escape onto a pooled connection.
    txn.execute_raw(Statement::from_string(
        DbBackend::Postgres,
        "SET LOCAL enable_seqscan = off".to_owned(),
    ))
    .await
    .expect("disable seqscan");

    let plan = txn
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "EXPLAIN SELECT t.id FROM threads t WHERE t.id IN ({})",
                ferum_infrastructure::search::postgres_fts::POST_MATCH_SUBQUERY
            ),
            [sea_orm::Value::from("camshaft:*")],
        ))
        .await
        .expect("explain")
        .iter()
        .map(|r| r.try_get::<String>("", "QUERY PLAN").expect("plan line"))
        .collect::<Vec<_>>()
        .join("\n");
    txn.commit().await.expect("commit");

    assert!(
        plan.contains("idx_posts_fts"),
        "the body predicate must reach the FTS index, not scan posts:\n{plan}"
    );
    db.teardown().await;
}

// ─── Products ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn product_search_finds_published_product_by_name() {
    let db = TestDb::new("fts_product_by_name").await;
    insert_product(&db.conn, "Milano Leather Sofa", "milano-sofa", ProductStatus::Published, None).await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let result = svc.search(product_query("sofa", None, 1, 20)).await.expect("search");
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].kind, SearchKind::Product);
    assert_eq!(result.hits[0].slug, "milano-sofa");
    db.teardown().await;
}

/// The whole reason `f_unaccent` exists: Vietnamese users type without tone
/// marks. Skipped, not failed, where the `unaccent` extension is unavailable —
/// the migration degrades to identity there by design.
#[tokio::test]
async fn product_search_matches_without_diacritics() {
    let db = TestDb::new("fts_product_unaccent").await;
    insert_product(&db.conn, "Ghế ăn Bắc Âu", "ghe-an-bac-au", ProductStatus::Published, None).await;

    use sea_orm::{ConnectionTrait, Statement};
    let unaccent_active: bool = db
        .conn
        .query_one_raw(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT (f_unaccent('ế') = 'e') AS folded".to_owned(),
        ))
        .await
        .expect("probe f_unaccent")
        .and_then(|r| r.try_get::<bool>("", "folded").ok())
        .unwrap_or(false);

    if !unaccent_active {
        eprintln!("unaccent extension unavailable — skipping diacritic-folding assertion");
        db.teardown().await;
        return;
    }

    let svc = PostgresFtsService::new(db.conn.clone());

    let bare = svc.search(product_query("ghe an", None, 1, 20)).await.expect("search");
    assert_eq!(bare.total, 1, "'ghe an' should match 'Ghế ăn'");

    // The other direction, and the one that actually shipped broken: the index
    // is folded, so an accented query must be folded too or it matches nothing.
    // Unaccenting only the column silently breaks search for anyone who types
    // Vietnamese properly — which is most people.
    let accented = svc.search(product_query("ghế", None, 1, 20)).await.expect("search");
    assert_eq!(accented.total, 1, "'ghế' should match 'Ghế ăn'");
    db.teardown().await;
}

/// Threads are indexed through the same folding, so the same failure applies.
#[tokio::test]
async fn thread_search_matches_an_accented_query() {
    let db = TestDb::new("fts_thread_accented").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;

    use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
    use ferum_infrastructure::repositories::PgThreadRepository;
    PgThreadRepository::new(db.conn.clone())
        .create(NewThread {
            id: Uuid::new_v4(),
            category_id: cat.id,
            author_id: user.id,
            title: "Đánh giá ghế công thái học".to_string(),
            slug: "danh-gia-ghe".to_string(),
            product_id: None,
        })
        .await
        .expect("create thread");

    let svc = PostgresFtsService::new(db.conn.clone());
    for q in ["ghế", "ghe", "đánh giá", "danh gia"] {
        let result = svc
            .search(thread_query(q, vec![cat.id], 1, 20))
            .await
            .expect("search");
        assert_eq!(result.total, 1, "query {q:?} should match the thread");
    }
    db.teardown().await;
}

#[tokio::test]
async fn product_search_hides_drafts_from_other_users() {
    let db = TestDb::new("fts_product_draft").await;
    let owner = insert_user(&db.conn, 1).await;
    let stranger = insert_user(&db.conn, 2).await;
    insert_product(&db.conn, "Pending Oak Table", "pending-oak", ProductStatus::Draft, Some(owner.id)).await;

    let svc = PostgresFtsService::new(db.conn.clone());

    let anon = svc.search(product_query("oak", None, 1, 20)).await.expect("anon");
    assert_eq!(anon.total, 0, "a draft is unlisted to guests");

    let other = svc.search(product_query("oak", Some(stranger.id), 1, 20)).await.expect("other");
    assert_eq!(other.total, 0, "a draft is unlisted to other members");

    let own = svc.search(product_query("oak", Some(owner.id), 1, 20)).await.expect("own");
    assert_eq!(own.total, 1, "the submitter can find their own pending entry");
    db.teardown().await;
}

// ─── Product facets ───────────────────────────────────────────────────────────

/// Each facet and each ordering builds different SQL — a wrong placeholder
/// index, a bad enum cast or a column that does not exist only fails when
/// Postgres parses it. This walks every branch against the live schema.
#[tokio::test]
async fn product_facet_and_sort_branches_all_execute() {
    let db = TestDb::new("fts_product_facets").await;
    let owner = insert_user(&db.conn, 1).await;
    insert_product(&db.conn, "Milano Leather Sofa", "milano-sofa", ProductStatus::Published, None)
        .await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let some_id = Uuid::new_v4();

    for sort in [
        ProductSearchSort::Relevance,
        ProductSearchSort::TopRated,
        ProductSearchSort::MostReviewed,
        ProductSearchSort::Newest,
    ] {
        for facets in [
            ProductFacets { sort, ..ProductFacets::default() },
            ProductFacets {
                sort,
                product_type: Some(ProductType::Furniture),
                ..ProductFacets::default()
            },
            ProductFacets { sort, brand_id: Some(some_id), ..ProductFacets::default() },
            ProductFacets { sort, material_id: Some(some_id), ..ProductFacets::default() },
            // All at once, with a viewer — the case with the most placeholders,
            // where an off-by-one in the parameter numbering would surface.
            ProductFacets {
                sort,
                product_type: Some(ProductType::Furniture),
                brand_id: Some(some_id),
                material_id: Some(some_id),
            },
        ] {
            let mut q = product_query("sofa", Some(owner.id), 1, 20);
            q.facets = facets.clone();
            svc.search(q)
                .await
                .unwrap_or_else(|e| panic!("sort {:?} facets {:?} failed: {e}", sort, facets));
        }
    }
    db.teardown().await;
}

/// A facet that matches nothing must narrow the result away rather than be
/// silently dropped — a filter that does nothing is worse than no filter.
#[tokio::test]
async fn product_facets_actually_narrow_the_result() {
    let db = TestDb::new("fts_product_facets_narrow").await;
    insert_product(&db.conn, "Milano Leather Sofa", "milano-sofa", ProductStatus::Published, None)
        .await;

    let svc = PostgresFtsService::new(db.conn.clone());

    let unfiltered = svc.search(product_query("sofa", None, 1, 20)).await.expect("search");
    assert_eq!(unfiltered.total, 1);

    // The fixture is Furniture, so filtering to Room must exclude it.
    let mut q = product_query("sofa", None, 1, 20);
    q.facets.product_type = Some(ProductType::Room);
    let wrong_type = svc.search(q).await.expect("search");
    assert_eq!(wrong_type.total, 0, "product_type facet must be applied");

    let mut q = product_query("sofa", None, 1, 20);
    q.facets.brand_id = Some(Uuid::new_v4());
    let wrong_brand = svc.search(q).await.expect("search");
    assert_eq!(wrong_brand.total, 0, "brand facet must be applied");

    let mut q = product_query("sofa", None, 1, 20);
    q.facets.material_id = Some(Uuid::new_v4());
    let wrong_material = svc.search(q).await.expect("search");
    assert_eq!(wrong_material.total, 0, "material facet must be applied");

    db.teardown().await;
}

// ─── Brand matching ───────────────────────────────────────────────────────────
//
// The brand name is no longer denormalised into `products.search_vector` — a
// generated column can only read its own row. These cover the semi-join that
// replaced it, and the staleness bug that disappeared with the copy.

async fn insert_brand(conn: &sea_orm::DatabaseConnection, name: &str, slug: &str) -> Uuid {
    PgBrandRepository::new(conn.clone())
        .create(NewBrand {
            id: Uuid::new_v4(),
            slug: slug.to_string(),
            name: name.to_string(),
            description: None,
            logo_url: None,
            website: None,
            country: None,
            is_verified: true,
            owner_user_id: None,
        })
        .await
        .expect("create brand")
        .id
}

async fn set_brand(conn: &sea_orm::DatabaseConnection, product_id: Uuid, brand_id: Uuid) {
    PgProductRepository::new(conn.clone())
        .update(
            product_id,
            UpdateProduct { brand_id: Some(Some(brand_id)), ..Default::default() },
        )
        .await
        .expect("attach brand");
}

#[tokio::test]
async fn product_search_finds_a_product_by_its_brand_name() {
    let db = TestDb::new("fts_product_brand_name").await;
    let brand = insert_brand(&db.conn, "Nhà Xinh", "nha-xinh").await;
    let product =
        insert_product(&db.conn, "Ghế bành bọc da", "ghe-banh", ProductStatus::Published, None)
            .await;
    set_brand(&db.conn, product, brand).await;

    let svc = PostgresFtsService::new(db.conn.clone());

    // Nothing in the product's own columns says "xinh" — only the brand does.
    let hits = svc.search(product_query("xinh", None, 1, 20)).await.expect("search");
    assert_eq!(hits.total, 1, "a brand-name query must reach the brand's products");
    assert_eq!(hits.hits[0].id, product);

    // And without tone marks, like the rest of search.
    let unaccented = svc.search(product_query("nha", None, 1, 20)).await.expect("search");
    assert_eq!(unaccented.total, 1, "brand matching must fold diacritics too");

    db.teardown().await;
}

#[tokio::test]
async fn renaming_a_brand_takes_effect_without_reindexing() {
    let db = TestDb::new("fts_brand_rename").await;
    let brand = insert_brand(&db.conn, "Oldname", "brand-rename").await;
    let product =
        insert_product(&db.conn, "Bàn trà gỗ sồi", "ban-tra", ProductStatus::Published, None).await;
    set_brand(&db.conn, product, brand).await;

    let svc = PostgresFtsService::new(db.conn.clone());
    assert_eq!(svc.search(product_query("oldname", None, 1, 20)).await.expect("search").total, 1);

    PgBrandRepository::new(db.conn.clone())
        .update(brand, UpdateBrand { name: Some("Newname".to_string()), ..Default::default() })
        .await
        .expect("rename brand");

    // The old name previously lingered in every one of the brand's product rows
    // until a second trigger rewrote them. Nothing is copied now, so the rename
    // is simply visible.
    assert_eq!(
        svc.search(product_query("newname", None, 1, 20)).await.expect("search").total,
        1,
        "the new brand name must match immediately"
    );
    assert_eq!(
        svc.search(product_query("oldname", None, 1, 20)).await.expect("search").total,
        0,
        "the old brand name must not still match"
    );
    assert_eq!(product, svc.search(product_query("newname", None, 1, 20)).await.unwrap().hits[0].id);

    db.teardown().await;
}

#[tokio::test]
async fn a_name_match_outranks_a_brand_only_match() {
    let db = TestDb::new("fts_brand_rank_order").await;
    let brand = insert_brand(&db.conn, "Milano", "milano-brand").await;

    // Matches through its own name (weight A).
    let by_name =
        insert_product(&db.conn, "Milano Sofa", "milano-sofa", ProductStatus::Published, None).await;
    // Matches only because it belongs to the Milano brand.
    let by_brand =
        insert_product(&db.conn, "Kệ sách gỗ", "ke-sach", ProductStatus::Published, None).await;
    set_brand(&db.conn, by_brand, brand).await;

    let svc = PostgresFtsService::new(db.conn.clone());
    let hits = svc.search(product_query("milano", None, 1, 20)).await.expect("search");

    assert_eq!(hits.total, 2, "both routes to a match must return a hit");
    assert_eq!(
        hits.hits[0].id, by_name,
        "a hit on the product's own name must outrank a hit on its brand"
    );
    assert_eq!(hits.hits[1].id, by_brand);

    db.teardown().await;
}

#[tokio::test]
async fn the_generated_column_follows_an_edit_with_no_trigger_involved() {
    let db = TestDb::new("fts_generated_column_follows").await;
    let product =
        insert_product(&db.conn, "Tủ quần áo", "tu-quan-ao", ProductStatus::Published, None).await;

    let svc = PostgresFtsService::new(db.conn.clone());
    assert_eq!(svc.search(product_query("tu", None, 1, 20)).await.expect("search").total, 1);

    PgProductRepository::new(db.conn.clone())
        .update(
            product,
            UpdateProduct {
                name: Some("Giường ngủ".to_string()),
                // No word here may begin with "tu" — queries are prefix
                // matched, so "túi" would keep the old query alive and the
                // assertion below would be testing nothing.
                description_md: Some(Some("Đệm lò xo độc lập".to_string())),
                ..Default::default()
            },
        )
        .await
        .expect("rename product");

    assert_eq!(
        svc.search(product_query("giuong", None, 1, 20)).await.expect("search").total,
        1,
        "the generated column must reflect the new name"
    );
    assert_eq!(
        svc.search(product_query("lo xo", None, 1, 20)).await.expect("search").total,
        1,
        "and the new description, which is weighted lower but still indexed"
    );
    assert_eq!(
        svc.search(product_query("tu", None, 1, 20)).await.expect("search").total,
        0,
        "the old name must no longer match"
    );

    db.teardown().await;
}
