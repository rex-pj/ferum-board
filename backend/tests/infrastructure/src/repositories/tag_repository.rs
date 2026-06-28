//! Integration tests for [`PgTagRepository`].
//!
//! Critical SQL: `find_by_threads` uses a JOIN on `thread_tags` returning a `HashMap<Uuid, Vec<Tag>>`.

use uuid::Uuid;
use ferum_domain::models::tag::NewTag;
use ferum_domain::repositories::tag_repository::TagRepository;
use ferum_infrastructure::repositories::PgTagRepository;

use crate::common::{insert_category, insert_thread, insert_user, TestDb};

fn new_tag(n: u8) -> NewTag {
    NewTag {
        id: Uuid::new_v4(),
        name: format!("tag{n}"),
        slug: format!("tag-{n}"),
        color: None,
        created_by_id: None,
    }
}

#[tokio::test]
async fn list_on_empty_schema_returns_empty() {
    let db = TestDb::new("tag_list_empty").await;
    let repo = PgTagRepository::new(db.conn.clone());
    let tags = repo.list(None, 20).await.expect("list");
    assert!(tags.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn create_and_find_by_slug() {
    let db = TestDb::new("tag_create_find").await;
    let repo = PgTagRepository::new(db.conn.clone());
    let tag = repo.create(new_tag(1)).await.expect("create");

    let found = repo.find_by_slug("tag-1").await.expect("find_by_slug").unwrap();
    assert_eq!(found.id, tag.id);
    assert_eq!(found.name, "tag1");
    db.teardown().await;
}

#[tokio::test]
async fn list_with_query_filters_by_name() {
    let db = TestDb::new("tag_list_query").await;
    let repo = PgTagRepository::new(db.conn.clone());
    repo.create(new_tag(1)).await.expect("create tag1");
    repo.create(NewTag { id: Uuid::new_v4(), name: "rust".to_string(), slug: "rust".to_string(), color: None, created_by_id: None }).await.expect("create rust");

    let results = repo.list(Some("rust"), 20).await.expect("list filtered");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "rust");
    db.teardown().await;
}

#[tokio::test]
async fn assign_to_thread_and_find_by_thread() {
    let db = TestDb::new("tag_assign_find_by_thread").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgTagRepository::new(db.conn.clone());
    let tag = repo.create(new_tag(1)).await.expect("create");
    repo.assign_to_thread(thread.id, &[tag.id]).await.expect("assign");

    let tags = repo.find_by_thread(thread.id).await.expect("find_by_thread");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].id, tag.id);
    db.teardown().await;
}

#[tokio::test]
async fn find_by_thread_no_tags_returns_empty() {
    let db = TestDb::new("tag_find_by_thread_empty").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgTagRepository::new(db.conn.clone());
    let tags = repo.find_by_thread(thread.id).await.expect("find_by_thread");
    assert!(tags.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn find_by_threads_empty_ids_returns_empty_map() {
    // Ensure the early-return path on empty slice executes cleanly.
    let db = TestDb::new("tag_find_by_threads_empty_ids").await;
    let repo = PgTagRepository::new(db.conn.clone());
    let result = repo.find_by_threads(&[]).await.expect("find_by_threads");
    assert!(result.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn find_by_threads_groups_tags_by_thread() {
    let db = TestDb::new("tag_find_by_threads_grouped").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread_a = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let thread_b = insert_thread(&db.conn, 2, cat.id, user.id).await;

    let repo = PgTagRepository::new(db.conn.clone());
    let tag_x = repo.create(new_tag(1)).await.expect("create x");
    let tag_y = repo.create(new_tag(2)).await.expect("create y");
    repo.assign_to_thread(thread_a.id, &[tag_x.id]).await.expect("assign x to a");
    repo.assign_to_thread(thread_b.id, &[tag_y.id]).await.expect("assign y to b");

    let result = repo.find_by_threads(&[thread_a.id, thread_b.id]).await.expect("find_by_threads");
    assert_eq!(result.get(&thread_a.id).unwrap()[0].id, tag_x.id);
    assert_eq!(result.get(&thread_b.id).unwrap()[0].id, tag_y.id);
    db.teardown().await;
}

#[tokio::test]
async fn replace_thread_tags_removes_old_and_adds_new() {
    let db = TestDb::new("tag_replace_thread_tags").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgTagRepository::new(db.conn.clone());
    let tag_a = repo.create(new_tag(1)).await.expect("create a");
    let tag_b = repo.create(new_tag(2)).await.expect("create b");
    repo.assign_to_thread(thread.id, &[tag_a.id]).await.expect("assign a");

    // Replace with just tag_b
    repo.replace_thread_tags(thread.id, &[tag_b.id]).await.expect("replace");
    let tags = repo.find_by_thread(thread.id).await.expect("find_by_thread");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].id, tag_b.id);
    db.teardown().await;
}
