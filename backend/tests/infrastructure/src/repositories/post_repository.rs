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
            slug: format!("thread-{n}"), product_id: None,
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
        .list_by_thread(Uuid::new_v4(), None, 1, 20)
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
        .list_pending(None, None, 1, 20)
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
        .list_pending(Some(Uuid::new_v4()), None, 1, 20)
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
async fn list_by_thread_includes_deleted_tombstones_and_own_pending() {
    let db = TestDb::new("post_list_by_thread_filters").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let author = make_user(&db.conn, 1).await;
    let other = make_user(&db.conn, 2).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, author.id, 1).await;

    // published: always visible
    make_post(&db.conn, thread.id, author.id, PostStatus::Published).await;
    // pending, by the author: visible only when viewer_id is the author
    let own_pending = make_post(&db.conn, thread.id, author.id, PostStatus::Pending).await;
    // pending, by someone else: never visible to `author`
    make_post(&db.conn, thread.id, other.id, PostStatus::Pending).await;
    // published then soft-deleted: still visible, as a tombstone (repo doesn't blank content — the caller does)
    let to_delete = make_post(&db.conn, thread.id, author.id, PostStatus::Published).await;
    repo.soft_delete(to_delete.id, author.id).await.expect("soft_delete");

    // Anonymous viewer: sees published + the tombstone, no pending posts at all.
    let (posts, total) = repo
        .list_by_thread(thread.id, None, 1, 20)
        .await
        .expect("list_by_thread with no viewer");
    assert_eq!(total, 2, "published post + deleted tombstone, no pending");
    assert!(posts.iter().all(|p| !matches!(p.status, PostStatus::Pending)));

    // The author viewing their own thread: also sees their own pending post.
    let (posts, total) = repo
        .list_by_thread(thread.id, Some(author.id), 1, 20)
        .await
        .expect("list_by_thread with viewer = author");
    assert_eq!(total, 3, "published + tombstone + author's own pending post");
    assert!(posts.iter().any(|p| p.id == own_pending.id));

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
        .list_pending(Some(cat_a.id), None, 1, 20)
        .await
        .expect("list_pending with category InnerJoin executes");

    assert_eq!(total, 1, "InnerJoin must restrict to posts in cat_a threads only");
    assert_eq!(posts[0].thread_id, thread_a.id);

    db.teardown().await;
}

/// The approval queue must never leak pending posts outside the categories a
/// moderator is assigned to, whichever `category_id` filter the client passes.
#[tokio::test]
async fn list_pending_allowlist_restricts_to_moderated_categories() {
    let db = TestDb::new("post_list_pending_allowlist").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat_a = make_category(&db.conn, "cat-a").await;
    let cat_b = make_category(&db.conn, "cat-b").await;
    let thread_a = make_thread(&db.conn, cat_a.id, user.id, 1).await;
    let thread_b = make_thread(&db.conn, cat_b.id, user.id, 2).await;

    make_post(&db.conn, thread_a.id, user.id, PostStatus::Pending).await;
    make_post(&db.conn, thread_b.id, user.id, PostStatus::Pending).await;

    // No filter + unrestricted (global moderator) sees both.
    let (_, total) = repo.list_pending(None, None, 1, 20).await.expect("unrestricted");
    assert_eq!(total, 2);

    // No filter, but allowlisted to cat_a: must see only cat_a's pending post.
    let (posts, total) = repo
        .list_pending(None, Some(&[cat_a.id]), 1, 20)
        .await
        .expect("allowlisted");
    assert_eq!(total, 1, "allowlist must clamp an unfiltered query");
    assert_eq!(posts[0].thread_id, thread_a.id);

    // Filter and allowlist are ANDed: asking for cat_b while only allowed cat_a
    // yields nothing (the use case additionally 403s before reaching here).
    let (_, total) = repo
        .list_pending(Some(cat_b.id), Some(&[cat_a.id]), 1, 20)
        .await
        .expect("filter AND allowlist");
    assert_eq!(total, 0, "filter must not escape the allowlist");

    // Empty allowlist must match nothing, not everything.
    let (_, total) = repo.list_pending(None, Some(&[]), 1, 20).await.expect("empty");
    assert_eq!(total, 0, "empty allowlist must fail closed");

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

// ─── content_md_by_thread (attachment release on thread delete) ───────────────

async fn make_post_with_content(
    db: &sea_orm::DatabaseConnection,
    thread_id: Uuid,
    author_id: Uuid,
    content_md: &str,
) -> ferum_domain::models::post::Post {
    PgPostRepository::new(db.clone())
        .create(NewPost {
            thread_id,
            author_id,
            parent_id: None,
            content_md: content_md.to_string(),
            content_html: format!("<p>{content_md}</p>"),
            status: PostStatus::Published,
        })
        .await
        .expect("create post with content")
}

#[tokio::test]
async fn content_md_by_thread_excludes_soft_deleted_posts() {
    let db = TestDb::new("post_content_by_thread").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;
    let other_thread = make_thread(&db.conn, cat.id, user.id, 2).await;

    make_post_with_content(&db.conn, thread.id, user.id, "keeps ![a](/files/x.png)").await;
    make_post_with_content(&db.conn, thread.id, user.id, "also kept").await;
    // Already released its own attachment refs when it was deleted — including it
    // here would decrement them a second time.
    let gone = make_post_with_content(&db.conn, thread.id, user.id, "deleted post").await;
    repo.soft_delete(gone.id, user.id).await.expect("soft_delete");
    // Belongs to a different thread; must not leak in.
    make_post_with_content(&db.conn, other_thread.id, user.id, "other thread").await;

    let mut contents = repo
        .content_md_by_thread(thread.id)
        .await
        .expect("content_md_by_thread");
    contents.sort();

    assert_eq!(
        contents,
        vec!["also kept".to_string(), "keeps ![a](/files/x.png)".to_string()],
        "only live posts of this thread"
    );

    db.teardown().await;
}

#[tokio::test]
async fn content_md_by_thread_returns_one_entry_per_post_not_a_set() {
    let db = TestDb::new("post_content_by_thread_dupes").await;
    let repo = PgPostRepository::new(db.conn.clone());

    let user = make_user(&db.conn, 1).await;
    let cat = make_category(&db.conn, "general").await;
    let thread = make_thread(&db.conn, cat.id, user.id, 1).await;

    // Two posts embedding the identical image hold two refs; both must come back
    // so thread deletion releases both.
    make_post_with_content(&db.conn, thread.id, user.id, "same body").await;
    make_post_with_content(&db.conn, thread.id, user.id, "same body").await;

    let contents = repo.content_md_by_thread(thread.id).await.expect("query");
    assert_eq!(contents.len(), 2, "identical bodies must not be collapsed");

    db.teardown().await;
}
