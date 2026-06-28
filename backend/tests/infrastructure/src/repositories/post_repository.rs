//! Integration tests for [`PgPostRepository`].
//!
//! These tests verify the conditional `InnerJoin` in `list_pending`, the
//! secondary JOIN to threads in `list_by_author`, and the `PostStatus` enum
//! cast filter used across all list queries.

use ferum_domain::models::category::{PostPolicy, ViewPolicy};
use ferum_domain::models::post::PostStatus;
use ferum_domain::repositories::category_repository::{CategoryRepository, NewCategory};
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
use ferum_domain::repositories::user_repository::{NewUser, UserRepository};
use ferum_infrastructure::repositories::{
    PgCategoryRepository, PgPostRepository, PgThreadRepository, PgUserRepository,
};
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
            slug: format!("thread-{n}"),
        })
        .await
        .expect("create test thread")
}

async fn make_post(
    db: &sea_orm::DatabaseConnection,
    thread_id: Uuid,
    author_id: Uuid,
    status: PostStatus,
) -> ferum_domain::models::post::Post {
    PgPostRepository::new(db.clone())
        .create(NewPost {
            thread_id,
            author_id,
            parent_id: None,
            content_md: "Hello world".to_string(),
            content_html: "<p>Hello world</p>".to_string(),
            status,
        })
        .await
        .expect("create test post")
}

// ─── Tests: read-only on empty schema ─────────────────────────────────────────

#[tokio::test]
async fn list_by_thread_on_empty_schema_returns_zero() {
    let db = TestDb::new("post_list_by_thread_empty").await;
    let repo = PgPostRepository::new(db.conn.clone());

    // Exercises PostStatus enum filter: .filter(status.eq(PostStatus::Published))
    let (posts, total) = repo
        .list_by_thread(Uuid::new_v4(), 1, 20)
        .await
        .expect("list_by_thread executes PostStatus filter on empty schema");
    assert_eq!(total, 0);
    assert!(posts.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn list_pending_without_category_filter_executes() {
    let db = TestDb::new("post_list_pending_no_cat").await;
    let repo = PgPostRepository::new(db.conn.clone());

    // list_pending with category_id = None: no InnerJoin applied
    let (posts, total) = repo
        .list_pending(None, 1, 20)
        .await
        .expect("list_pending without category filter executes");
    assert_eq!(total, 0);
    assert!(posts.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn list_pending_with_category_filter_inner_join_executes() {
    let db = TestDb::new("post_list_pending_with_cat").await;
    let repo = PgPostRepository::new(db.conn.clone());

    // list_pending with category_id = Some: InnerJoin with threads table
    let (posts, total) = repo
        .list_pending(Some(Uuid::new_v4()), 1, 20)
        .await
        .expect("list_pending with InnerJoin to threads executes on empty schema");
    assert_eq!(total, 0);
    assert!(posts.is_empty());

    db.teardown().await;
}

// ─── Tests: create → query roundtrip ──────────────────────────────────────────

#[tokio::test]
async fn create_and_find_by_id_roundtrip() {
    let db = TestDb::new("post_create_find").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    let post = make_post(&db.conn, thread.id, user.id, PostStatus::Published).await;

    let found = repo
        .find_by_id(post.id)
        .await
        .expect("find_by_id executes")
        .expect("post should exist");

    assert_eq!(found.id, post.id);
    assert_eq!(found.thread_id, thread.id);
    assert!(matches!(found.status, PostStatus::Published));

    db.teardown().await;
}

#[tokio::test]
async fn list_by_thread_filters_deleted_and_pending() {
    let db = TestDb::new("post_list_by_thread_filters").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    // published: should appear
    make_post(&db.conn, thread.id, user.id, PostStatus::Published).await;
    // pending: should be filtered out
    make_post(&db.conn, thread.id, user.id, PostStatus::Pending).await;
    // published then soft-deleted
    let to_delete = make_post(&db.conn, thread.id, user.id, PostStatus::Published).await;
    repo.soft_delete(to_delete.id, user.id).await.expect("soft_delete");

    let (posts, total) = repo
        .list_by_thread(thread.id, 1, 20)
        .await
        .expect("list_by_thread with filters");

    assert_eq!(total, 1, "only the published, non-deleted post should appear");
    assert_eq!(posts.len(), 1);

    db.teardown().await;
}

#[tokio::test]
async fn list_by_author_includes_thread_context() {
    let db = TestDb::new("post_list_by_author_thread_ctx").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    make_post(&db.conn, thread.id, user.id, PostStatus::Published).await;

    // list_by_author does a secondary SELECT against threads to get slug + title
    let (posts, total) = repo
        .list_by_author(user.id, 1, 20)
        .await
        .expect("list_by_author with thread JOIN executes");

    assert_eq!(total, 1);
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].thread_slug.as_deref(), Some("thread-1"),
        "thread_slug must be populated from secondary JOIN");
    assert_eq!(posts[0].thread_title.as_deref(), Some("Thread 1"),
        "thread_title must be populated from secondary JOIN");

    db.teardown().await;
}

#[tokio::test]
async fn list_pending_with_category_returns_only_matching_thread_posts() {
    let db = TestDb::new("post_list_pending_cat_filter").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat_a = make_category(&db.conn, "cat-a").await;
    let cat_b = make_category(&db.conn, "cat-b").await;
    let thread_a = make_thread(&db.conn, cat_a.id, user.id, 1).await;
    let thread_b = make_thread(&db.conn, cat_b.id, user.id, 2).await;

    make_post(&db.conn, thread_a.id, user.id, PostStatus::Pending).await;
    make_post(&db.conn, thread_b.id, user.id, PostStatus::Pending).await;

    // list_pending with cat_a: InnerJoin should limit to posts from thread_a only
    let (posts, total) = repo
        .list_pending(Some(cat_a.id), 1, 20)
        .await
        .expect("list_pending with category InnerJoin executes");

    assert_eq!(total, 1, "InnerJoin must restrict to posts in cat_a threads only");
    assert_eq!(posts[0].thread_id, thread_a.id);

    db.teardown().await;
}

#[tokio::test]
async fn update_content_increments_edit_count() {
    let db = TestDb::new("post_update_content").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;
    let post = make_post(&db.conn, thread.id, user.id, PostStatus::Published).await;

    let updated = repo
        .update_content(post.id, "edited".to_string(), "<p>edited</p>".to_string(), user.id)
        .await
        .expect("update_content executes");

    assert_eq!(updated.edit_count, 1);
    assert_eq!(updated.content_md, "edited");
    assert!(updated.edited_at.is_some());

    db.teardown().await;
}

#[tokio::test]
async fn set_status_changes_post_status() {
    let db = TestDb::new("post_set_status").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;
    let post = make_post(&db.conn, thread.id, user.id, PostStatus::Pending).await;

    repo.set_status(post.id, PostStatus::Published)
        .await
        .expect("set_status executes");

    let found = repo
        .find_by_id(post.id)
        .await
        .expect("find_by_id")
        .expect("post should exist");
    assert!(matches!(found.status, PostStatus::Published));

    db.teardown().await;
}
