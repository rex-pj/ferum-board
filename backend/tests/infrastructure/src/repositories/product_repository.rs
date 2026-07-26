//! Integration tests for [`PgProductRepository`].
//!
//! Critical SQL: the `TopRated` ordering. It is a hand-written `Expr::cust_with_values`
//! CASE expression — the type checker cannot see inside it, so nothing else in the
//! suite would catch a placeholder that binds the wrong value, a numeric/float type
//! error, or a NULL guard that stops working. These tests pin the *behaviour* the
//! expression exists for: a product cannot reach the top of a review platform on a
//! single glowing review.

use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::json;
use uuid::Uuid;

use ferum_domain::models::product::{NewProduct, ProductStatus, ProductType};
use ferum_domain::repositories::product_repository::{
    ProductListFilter, ProductRepository, ProductSort,
};
use ferum_infrastructure::repositories::PgProductRepository;

use crate::common::TestDb;

fn new_product(slug: &str, name: &str) -> NewProduct {
    NewProduct {
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
        dimensions: json!({}),
        origin: None,
        description_md: None,
        created_by_id: None,
        material_ids: Vec::new(),
    }
}

/// `product_rating_stats` is a plain rollup table the application recomputes on
/// review write (see migration 30), so a test can seed it directly instead of
/// staging whole review threads just to reach a number.
async fn set_stats(db: &TestDb, product_id: Uuid, review_count: i32, avg_overall: f64) {
    db.conn
        .execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            // `$3::numeric` rather than binding a Decimal: it keeps the column's
            // real type without pulling rust_decimal into the test crate.
            "INSERT INTO product_rating_stats (product_id, review_count, avg_overall, updated_at) \
             VALUES ($1, $2, $3::numeric, now())",
            [product_id.into(), review_count.into(), avg_overall.into()],
        ))
        .await
        .expect("seed product_rating_stats");
}

async fn published(repo: &PgProductRepository, p: NewProduct) -> Uuid {
    let created = repo.create(p).await.expect("create product");
    repo.update(
        created.id,
        ferum_domain::repositories::product_repository::UpdateProduct {
            status: Some(ProductStatus::Published),
            ..Default::default()
        },
    )
    .await
    .expect("publish product");
    created.id
}

fn top_rated_filter() -> ProductListFilter {
    ProductListFilter {
        status: Some(ProductStatus::Published),
        sort: ProductSort::TopRated,
        ..Default::default()
    }
}

/// The defect this ordering exists to prevent: a lone 5★ review outranking a
/// strong average backed by real volume.
#[tokio::test]
async fn top_rated_does_not_let_one_review_outrank_a_well_reviewed_product() {
    let db = TestDb::new("product_top_rated_shrinkage").await;
    let repo = PgProductRepository::new(db.conn.clone());

    let lucky = published(&repo, new_product("lucky", "One perfect review")).await;
    let proven = published(&repo, new_product("proven", "Proven over time")).await;

    set_stats(&db, lucky, 1, 5.0).await;
    set_stats(&db, proven, 200, 4.6).await;

    let (items, _) = repo.list(top_rated_filter(), 1, 10).await.expect("list");
    let order: Vec<&str> = items.iter().map(|i| i.product.slug.as_str()).collect();

    // Raw `avg_overall DESC` would put "lucky" (5.0) first. Shrunk:
    //   lucky  = (5.0×1   + 3.5×10) / (1   + 10) = 3.64
    //   proven = (4.6×200 + 3.5×10) / (200 + 10) = 4.55
    assert_eq!(
        order,
        vec!["proven", "lucky"],
        "a single 5★ review must not outrank a 4.6★ average over 200 reviews"
    );
    db.teardown().await;
}

/// The prior pulls thin evidence toward the middle from *both* directions: a
/// single 1★ must not sink a product below a genuinely mediocre one either.
#[tokio::test]
async fn top_rated_shrinks_thin_evidence_upward_too() {
    let db = TestDb::new("product_top_rated_shrink_up").await;
    let repo = PgProductRepository::new(db.conn.clone());

    let unlucky = published(&repo, new_product("unlucky", "One angry review")).await;
    let mediocre = published(&repo, new_product("mediocre", "Consistently mediocre")).await;

    set_stats(&db, unlucky, 1, 1.0).await;
    set_stats(&db, mediocre, 80, 3.0).await;

    let (items, _) = repo.list(top_rated_filter(), 1, 10).await.expect("list");
    let order: Vec<&str> = items.iter().map(|i| i.product.slug.as_str()).collect();

    //   unlucky  = (1.0×1  + 3.5×10) / (1  + 10) = 3.27
    //   mediocre = (3.0×80 + 3.5×10) / (80 + 10) = 3.06
    assert_eq!(
        order,
        vec!["unlucky", "mediocre"],
        "one bad review is not evidence enough to rank below a proven 3.0 average"
    );
    db.teardown().await;
}

/// The CASE guard: an unrated product has no score, so it must sort last rather
/// than inherit the 3.5 prior and land above products that genuinely scored below it.
#[tokio::test]
async fn top_rated_sorts_unrated_products_last_not_at_the_prior() {
    let db = TestDb::new("product_top_rated_unrated").await;
    let repo = PgProductRepository::new(db.conn.clone());

    let unrated = published(&repo, new_product("unrated", "Never reviewed")).await;
    let poor = published(&repo, new_product("poor", "Genuinely poor")).await;
    let _ = unrated;

    // `poor` scores 2.0 over 40 reviews → shrunk to 2.30, below the 3.5 prior.
    // If the CASE were dropped, `unrated` would score 3.5 and jump ahead of it.
    set_stats(&db, poor, 40, 2.0).await;

    let (items, _) = repo.list(top_rated_filter(), 1, 10).await.expect("list");
    let order: Vec<&str> = items.iter().map(|i| i.product.slug.as_str()).collect();

    assert_eq!(
        order,
        vec!["poor", "unrated"],
        "an unrated product must sort last, not at the prior's value"
    );
    db.teardown().await;
}

/// `min_review_count` is eligibility, not ordering — and it must narrow the
/// reported total too, or the caller pages through rows that aren't there.
#[tokio::test]
async fn min_review_count_filters_rows_and_total_together() {
    let db = TestDb::new("product_min_review_count").await;
    let repo = PgProductRepository::new(db.conn.clone());

    let thin = published(&repo, new_product("thin", "Two reviews")).await;
    let solid = published(&repo, new_product("solid", "Five reviews")).await;
    let _unrated = published(&repo, new_product("none", "No reviews")).await;

    set_stats(&db, thin, 2, 4.9).await;
    set_stats(&db, solid, 5, 4.0).await;

    let (items, total) = repo
        .list(
            ProductListFilter {
                min_review_count: Some(3),
                ..top_rated_filter()
            },
            1,
            10,
        )
        .await
        .expect("list");

    let slugs: Vec<&str> = items.iter().map(|i| i.product.slug.as_str()).collect();
    assert_eq!(slugs, vec!["solid"], "only products at or above the floor");
    assert_eq!(total, 1, "total must reflect the floor, not the unfiltered set");
    db.teardown().await;
}

/// Without a floor, nothing is hidden — plain browsing still sees everything.
#[tokio::test]
async fn no_min_review_count_keeps_unrated_products_listed() {
    let db = TestDb::new("product_no_floor").await;
    let repo = PgProductRepository::new(db.conn.clone());

    let rated = published(&repo, new_product("rated", "Rated")).await;
    let _unrated = published(&repo, new_product("unrated", "Unrated")).await;
    set_stats(&db, rated, 4, 4.2).await;

    let (items, total) = repo.list(top_rated_filter(), 1, 10).await.expect("list");
    assert_eq!(items.len(), 2);
    assert_eq!(total, 2);
    db.teardown().await;
}
