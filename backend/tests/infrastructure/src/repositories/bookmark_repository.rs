//! Integration tests for [`PgBookmarkRepository`].

use ferum_domain::repositories::bookmark_repository::BookmarkRepository;
use ferum_infrastructure::repositories::PgBookmarkRepository;

use crate::common::{insert_category, insert_thread, insert_user, TestDb};

#[tokio::test]
async fn find_on_empty_returns_none() {
    let db = TestDb::new("bk_find_empty").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgBookmarkRepository::new(db.conn.clone());
    let found = repo.find(user.id, thread.id).await.expect("find");
    assert!(found.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn add_and_find_bookmark() {
    let db = TestDb::new("bk_add_find").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgBookmarkRepository::new(db.conn.clone());
    repo.add(user.id, thread.id).await.expect("add");

    let found = repo.find(user.id, thread.id).await.expect("find").unwrap();
    assert_eq!(found.user_id, user.id);
    assert_eq!(found.thread_id, thread.id);
    db.teardown().await;
}

#[tokio::test]
async fn remove_bookmark_makes_find_return_none() {
    let db = TestDb::new("bk_remove").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgBookmarkRepository::new(db.conn.clone());
    repo.add(user.id, thread.id).await.expect("add");
    repo.remove(user.id, thread.id).await.expect("remove");

    let found = repo.find(user.id, thread.id).await.expect("find");
    assert!(found.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn list_for_user_returns_bookmarked_threads() {
    let db = TestDb::new("bk_list_for_user").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread_a = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let thread_b = insert_thread(&db.conn, 2, cat.id, user.id).await;

    let repo = PgBookmarkRepository::new(db.conn.clone());
    repo.add(user.id, thread_a.id).await.expect("add a");
    repo.add(user.id, thread_b.id).await.expect("add b");

    let (bookmarks, total) = repo.list_for_user(user.id, 1, 20).await.expect("list_for_user");
    assert_eq!(total, 2);
    assert_eq!(bookmarks.len(), 2);
    db.teardown().await;
}

#[tokio::test]
async fn duplicate_bookmark_does_not_add_twice() {
    let db = TestDb::new("bk_duplicate").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;

    let repo = PgBookmarkRepository::new(db.conn.clone());
    repo.add(user.id, thread.id).await.expect("first add");
    // Second add should fail with Conflict or the unique constraint must handle it
    let result = repo.add(user.id, thread.id).await;
    assert!(result.is_err(), "duplicate bookmark must not be silently inserted");
    db.teardown().await;
}
