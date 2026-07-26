//! Integration tests for the catalogue taxonomy and its auto-assign matcher.
//!
//! The matcher is the reason this taxonomy exists at all: filing a few hundred
//! products by hand was the chore that made the previous design unusable. So
//! the matcher's *rules* are what get tested here, against real Postgres —
//! `f_unaccent`, `~ '\m…\M'` word boundaries and `DISTINCT ON` ordering are all
//! database behaviour that no unit test can stand in for.

use ferum_domain::models::product::{NewProduct, ProductStatus, ProductType};
use ferum_domain::models::product_category::{NewProductCategory, UpdateProductCategory};
use ferum_domain::repositories::product_repository::{
    ProductCategoryRepository, ProductRepository, UpdateProduct,
};
use ferum_infrastructure::repositories::{PgProductCategoryRepository, PgProductRepository};
use uuid::Uuid;

use crate::common::TestDb;

async fn add_product(conn: &sea_orm::DatabaseConnection, name: &str, slug: &str) -> Uuid {
    PgProductRepository::new(conn.clone())
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
            created_by_id: None,
            material_ids: vec![],
        })
        .await
        .expect("create product")
        .id
}

async fn category_of(conn: &sea_orm::DatabaseConnection, product_id: Uuid) -> Option<Uuid> {
    PgProductRepository::new(conn.clone())
        .find_by_id(product_id)
        .await
        .expect("find product")
        .expect("product exists")
        .category_id
}

async fn slug_of(conn: &sea_orm::DatabaseConnection, category_id: Uuid) -> String {
    PgProductCategoryRepository::new(conn.clone())
        .find_by_id(category_id)
        .await
        .expect("find category")
        .expect("category exists")
        .slug
}

/// The migration must ship a usable taxonomy, not an empty table — an empty one
/// makes every downstream filter dead on arrival.
#[tokio::test]
async fn migration_seeds_the_default_taxonomy() {
    let db = TestDb::new("pcat_seed").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());

    let cats = repo.list().await.expect("list");
    let slugs: Vec<&str> = cats.iter().map(|c| c.slug.as_str()).collect();

    for expected in ["sofa", "ghe", "ban", "tu-ke", "giuong-nem", "den", "trang-tri"] {
        assert!(slugs.contains(&expected), "missing seeded category {expected}");
    }
    assert!(
        cats.iter().all(|c| !c.match_keywords.is_empty()),
        "a category with no keywords can never be auto-assigned"
    );
    db.teardown().await;
}

/// The migration backfills on the way in, so products that already existed are
/// filed without anyone lifting a finger. (Here the products are created after
/// the migration, so this covers the repository's matcher on the same rules.)
#[tokio::test]
async fn auto_assign_files_products_by_the_word_in_their_name() {
    let db = TestDb::new("pcat_auto_assign").await;
    let sofa = add_product(&db.conn, "Sofa da Milano", "milano-sofa").await;
    let chair = add_product(&db.conn, "Ghế ăn Bắc Âu", "ghe-an-bac-au").await;
    let table = add_product(&db.conn, "Bàn trà gỗ sồi", "ban-tra-go-soi").await;
    // No keyword matches this one; it must stay unfiled rather than be guessed.
    let mystery = add_product(&db.conn, "Milano X2", "milano-x2").await;

    let repo = PgProductCategoryRepository::new(db.conn.clone());
    let report = repo.auto_assign_categories(false).await.expect("auto assign");

    assert_eq!(report.assigned, 3);
    assert_eq!(report.unmatched, 1);

    assert_eq!(slug_of(&db.conn, category_of(&db.conn, sofa).await.unwrap()).await, "sofa");
    assert_eq!(slug_of(&db.conn, category_of(&db.conn, chair).await.unwrap()).await, "ghe");
    assert_eq!(slug_of(&db.conn, category_of(&db.conn, table).await.unwrap()).await, "ban");
    assert_eq!(category_of(&db.conn, mystery).await, None, "no guess without evidence");

    db.teardown().await;
}

/// Accent-blind, like every other text match on this site: a curator typing
/// "ghe" and a product named "Ghế" have to meet.
#[tokio::test]
async fn auto_assign_ignores_diacritics() {
    let db = TestDb::new("pcat_unaccent").await;
    let a = add_product(&db.conn, "Ghế xoay văn phòng", "ghe-xoay").await;
    let b = add_product(&db.conn, "Ghe go cao cap", "ghe-go").await;

    PgProductCategoryRepository::new(db.conn.clone())
        .auto_assign_categories(false)
        .await
        .expect("auto assign");

    for id in [a, b] {
        assert_eq!(
            slug_of(&db.conn, category_of(&db.conn, id).await.expect("assigned")).await,
            "ghe"
        );
    }
    db.teardown().await;
}

/// The word-boundary rule, stated as a test. Without `\m…\M` a substring match
/// files a keyboard ("Bàn phím") under tables via "ban" — and worse, any name
/// merely *containing* those letters.
#[tokio::test]
async fn auto_assign_matches_whole_words_only() {
    let db = TestDb::new("pcat_word_boundary").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());

    // "banquette" contains "ban" but is not a "bàn".
    let decoy = add_product(&db.conn, "Banquette Milano", "banquette-milano").await;
    let real = add_product(&db.conn, "Bàn ăn mở rộng", "ban-an").await;

    repo.auto_assign_categories(false).await.expect("auto assign");

    assert_eq!(
        category_of(&db.conn, decoy).await,
        None,
        "a substring is not a word — 'banquette' must not become a table"
    );
    assert_eq!(slug_of(&db.conn, category_of(&db.conn, real).await.unwrap()).await, "ban");
    db.teardown().await;
}

/// When two keywords match, the longer (more specific) one wins — otherwise
/// "Ghế sofa" lands under Ghế and the more precise answer is thrown away.
#[tokio::test]
async fn auto_assign_prefers_the_most_specific_keyword() {
    let db = TestDb::new("pcat_specificity").await;
    let p = add_product(&db.conn, "Ghế sofa phòng khách", "ghe-sofa").await;

    PgProductCategoryRepository::new(db.conn.clone())
        .auto_assign_categories(false)
        .await
        .expect("auto assign");

    assert_eq!(
        slug_of(&db.conn, category_of(&db.conn, p).await.expect("assigned")).await,
        "sofa",
        "'ghe sofa' is more specific than 'ghe'"
    );
    db.teardown().await;
}

/// A dry run must report the same numbers it would have written, and write
/// nothing. This is what the admin UI shows before a bulk write over the whole
/// catalogue.
#[tokio::test]
async fn dry_run_reports_without_writing() {
    let db = TestDb::new("pcat_dry_run").await;
    let p = add_product(&db.conn, "Sofa da Milano", "milano-sofa").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());

    let preview = repo.auto_assign_categories(true).await.expect("dry run");
    assert_eq!(preview.assigned, 1);
    assert_eq!(category_of(&db.conn, p).await, None, "dry run must not write");

    let applied = repo.auto_assign_categories(false).await.expect("apply");
    assert_eq!(applied.assigned, preview.assigned, "preview must match reality");
    assert!(category_of(&db.conn, p).await.is_some());
    db.teardown().await;
}

/// Re-running must be a no-op, and must never overwrite a decision a human
/// made — the matcher only ever fills blanks.
#[tokio::test]
async fn auto_assign_never_overwrites_an_existing_assignment() {
    let db = TestDb::new("pcat_idempotent").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());
    let products = PgProductRepository::new(db.conn.clone());

    // Named like a sofa, but a curator filed it under Ghế on purpose.
    let p = add_product(&db.conn, "Sofa da Milano", "milano-sofa").await;
    let ghe = repo
        .list()
        .await
        .expect("list")
        .into_iter()
        .find(|c| c.slug == "ghe")
        .expect("seeded");
    products
        .update(
            p,
            UpdateProduct { category_id: Some(Some(ghe.id)), ..Default::default() },
        )
        .await
        .expect("manual assignment");

    let report = repo.auto_assign_categories(false).await.expect("auto assign");

    assert_eq!(report.assigned, 0, "nothing left to fill");
    assert_eq!(
        category_of(&db.conn, p).await,
        Some(ghe.id),
        "the curator's decision stands"
    );
    db.teardown().await;
}

/// Keywords are data, so an admin can teach the matcher a word and re-run it
/// without a deploy. That is the whole reason they live in a column.
#[tokio::test]
async fn editing_keywords_changes_what_the_matcher_finds() {
    let db = TestDb::new("pcat_keywords_editable").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());
    let p = add_product(&db.conn, "Divan Milano", "divan-milano").await;

    assert_eq!(
        repo.auto_assign_categories(false).await.expect("first pass").assigned,
        0,
        "'divan' is not a seeded keyword yet"
    );

    let sofa = repo
        .list()
        .await
        .expect("list")
        .into_iter()
        .find(|c| c.slug == "sofa")
        .expect("seeded");
    let mut keywords = sofa.match_keywords.clone();
    keywords.push("divan".to_string());
    repo.update(
        sofa.id,
        UpdateProductCategory { match_keywords: Some(keywords), ..Default::default() },
    )
    .await
    .expect("teach the matcher");

    assert_eq!(
        repo.auto_assign_categories(false).await.expect("second pass").assigned,
        1
    );
    assert_eq!(slug_of(&db.conn, category_of(&db.conn, p).await.unwrap()).await, "sofa");
    db.teardown().await;
}

/// The unfiled bucket is the remaining manual queue, so it has to be countable.
#[tokio::test]
async fn product_counts_include_the_unfiled_bucket() {
    let db = TestDb::new("pcat_counts").await;
    add_product(&db.conn, "Sofa da Milano", "milano-sofa").await;
    add_product(&db.conn, "Milano X2", "milano-x2").await;

    let repo = PgProductCategoryRepository::new(db.conn.clone());
    repo.auto_assign_categories(false).await.expect("auto assign");

    let counts = repo.product_counts().await.expect("counts");
    assert_eq!(counts.get(&None).copied(), Some(1), "one product still unfiled");
    assert_eq!(counts.values().sum::<u64>(), 2);
    db.teardown().await;
}

/// Deleting a category unfiles its products rather than deleting them — a
/// category is a label, and losing the label must not lose the product.
#[tokio::test]
async fn deleting_a_category_unfiles_its_products() {
    let db = TestDb::new("pcat_delete").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());
    let p = add_product(&db.conn, "Sofa da Milano", "milano-sofa").await;
    repo.auto_assign_categories(false).await.expect("auto assign");

    let assigned = category_of(&db.conn, p).await.expect("assigned");
    repo.delete(assigned).await.expect("delete category");

    assert_eq!(category_of(&db.conn, p).await, None, "product survives, unfiled");
    assert!(
        PgProductRepository::new(db.conn.clone())
            .find_by_id(p)
            .await
            .expect("find")
            .is_some(),
        "the product itself must not be cascaded away"
    );
    db.teardown().await;
}

#[tokio::test]
async fn create_and_update_round_trip() {
    let db = TestDb::new("pcat_crud").await;
    let repo = PgProductCategoryRepository::new(db.conn.clone());

    let created = repo
        .create(NewProductCategory {
            id: Uuid::new_v4(),
            slug: "phu-kien".into(),
            name: "Phụ kiện".into(),
            parent_id: None,
            position: 99,
            icon: Some("fa-box".into()),
            match_keywords: vec!["phu kien".into()],
        })
        .await
        .expect("create");

    let updated = repo
        .update(
            created.id,
            UpdateProductCategory {
                name: Some("Phụ kiện & Đồ trang trí".into()),
                position: Some(50),
                ..Default::default()
            },
        )
        .await
        .expect("update");

    assert_eq!(updated.name, "Phụ kiện & Đồ trang trí");
    assert_eq!(updated.position, 50);
    assert_eq!(updated.slug, "phu-kien", "slug is a stable key");
    assert_eq!(updated.match_keywords, vec!["phu kien".to_string()], "untouched fields survive");
    db.teardown().await;
}

/// Status is irrelevant to filing — a draft submission still needs a category,
/// or it arrives in the curator's queue with nothing to review.
#[tokio::test]
async fn auto_assign_covers_drafts_too() {
    let db = TestDb::new("pcat_drafts").await;
    let p = add_product(&db.conn, "Sofa da Milano", "milano-sofa").await;
    PgProductRepository::new(db.conn.clone())
        .update(p, UpdateProduct { status: Some(ProductStatus::Draft), ..Default::default() })
        .await
        .expect("to draft");

    PgProductCategoryRepository::new(db.conn.clone())
        .auto_assign_categories(false)
        .await
        .expect("auto assign");

    assert!(category_of(&db.conn, p).await.is_some());
    db.teardown().await;
}
