//! Integration tests for [`PgCategoryRepository`].
//!
//! Critical SQL: `count_threads_by_categories` uses COALESCE + SUM(CASE) + LEFT JOIN.

use ferum_domain::models::category::{PostPolicy, ViewPolicy};
use ferum_domain::repositories::category_repository::{CategoryRepository, NewCategory, UpdateCategory};
use ferum_infrastructure::repositories::PgCategoryRepository;

use crate::common::{insert_category, insert_thread, insert_user, TestDb};

fn new_cat(n: u8) -> NewCategory {
    NewCategory {
        slug: format!("cat-{n}"),
        name: format!("Category {n}"),
        description: None,
        parent_id: None,
        position: n as i32,
        view_policy: ViewPolicy::Public,
        post_policy: PostPolicy::Members,
        color: None,
        created_by_id: None,
    }
}

#[tokio::test]
async fn list_all_on_empty_schema_returns_empty() {
    let db = TestDb::new("cat_list_empty").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let cats = repo.list_all().await.expect("list_all");
    assert!(cats.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn create_and_find_by_slug() {
    let db = TestDb::new("cat_create_find_slug").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let cat = repo.create(new_cat(1)).await.expect("create");

    let found = repo.find_by_slug("cat-1").await.expect("find_by_slug").unwrap();
    assert_eq!(found.id, cat.id);
    assert_eq!(found.name, "Category 1");
    assert_eq!(found.view_policy, ViewPolicy::Public);
    db.teardown().await;
}

#[tokio::test]
async fn find_by_slug_missing_returns_none() {
    let db = TestDb::new("cat_find_slug_none").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let res = repo.find_by_slug("nonexistent").await.expect("find_by_slug");
    assert!(res.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn update_patches_fields() {
    let db = TestDb::new("cat_update_fields").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let cat = repo.create(new_cat(1)).await.expect("create");

    let updated = repo.update(cat.id, UpdateCategory {
        name: Some("Updated Name".to_string()),
        view_policy: Some(ViewPolicy::MembersOnly),
        ..Default::default()
    }).await.expect("update");

    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.view_policy, ViewPolicy::MembersOnly);
    // unchanged fields preserved
    assert_eq!(updated.slug, "cat-1");
    db.teardown().await;
}

#[tokio::test]
async fn delete_removes_category() {
    let db = TestDb::new("cat_delete").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let cat = repo.create(new_cat(1)).await.expect("create");

    repo.delete(cat.id).await.expect("delete");
    let found = repo.find_by_id(cat.id).await.expect("find_by_id");
    assert!(found.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn has_children_true_when_subcategory_exists() {
    let db = TestDb::new("cat_has_children").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let parent = repo.create(new_cat(1)).await.expect("create parent");

    repo.create(NewCategory {
        parent_id: Some(parent.id),
        slug: "sub-1".to_string(),
        name: "Sub 1".to_string(),
        description: None,
        position: 0,
        view_policy: ViewPolicy::Public,
        post_policy: PostPolicy::Members,
        color: None,
        created_by_id: None,
    }).await.expect("create child");

    assert!(repo.has_children(parent.id).await.expect("has_children"));
    db.teardown().await;
}

#[tokio::test]
async fn has_children_false_for_leaf_category() {
    let db = TestDb::new("cat_no_children").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let cat = repo.create(new_cat(1)).await.expect("create");
    assert!(!repo.has_children(cat.id).await.expect("has_children"));
    db.teardown().await;
}

#[tokio::test]
async fn count_threads_by_categories_empty_list_returns_empty() {
    let db = TestDb::new("cat_count_threads_empty_ids").await;
    let repo = PgCategoryRepository::new(db.conn.clone());
    let result = repo.count_threads_by_categories(&[]).await.expect("count_threads");
    assert!(result.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn count_threads_by_categories_zero_for_empty_category() {
    // COALESCE(SUM(CASE ...), 0) must produce 0 when no threads exist.
    let db = TestDb::new("cat_count_threads_zero").await;
    let cat = insert_category(&db.conn, 1).await;

    let repo = PgCategoryRepository::new(db.conn.clone());
    let counts = repo.count_threads_by_categories(&[cat.id]).await.expect("count_threads");
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].1, 0, "empty category should count 0");
    db.teardown().await;
}

#[tokio::test]
async fn count_threads_by_categories_counts_published_threads() {
    let db = TestDb::new("cat_count_threads_published").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    insert_thread(&db.conn, 1, cat.id, user.id).await;
    insert_thread(&db.conn, 2, cat.id, user.id).await;

    let repo = PgCategoryRepository::new(db.conn.clone());
    let counts = repo.count_threads_by_categories(&[cat.id]).await.expect("count_threads");
    assert_eq!(counts[0].1, 2);
    db.teardown().await;
}
