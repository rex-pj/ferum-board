use std::sync::Arc;

use ferum_application::usecases::bookmark_usecase::BookmarkUseCase;
use ferum_domain::AppError;
use ferum_test_support::fixtures::{ids, make_bookmark, make_thread, AuthUserBuilder};
use ferum_test_support::mocks::{
    bookmark_repository::MockBookmarkRepository,
    thread_repository::MockThreadRepository,
};

// ─── Builder ──────────────────────────────────────────────────────────────────

fn build(bookmarks: MockBookmarkRepository, threads: MockThreadRepository) -> BookmarkUseCase {
    BookmarkUseCase::new(Arc::new(bookmarks), Arc::new(threads))
}

// ─── add ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();
    let (bm, th) = (MockBookmarkRepository::new(), MockThreadRepository::new());
    let result = build(bm, th).add(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn add_thread_not_found_returns_not_found() {
    let actor = AuthUserBuilder::member().build();
    let mut th = MockThreadRepository::new();
    th.expect_find_by_id().return_once(|_| Ok(None));

    let result = build(MockBookmarkRepository::new(), th).add(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn add_deleted_thread_returns_not_found() {
    let actor = AuthUserBuilder::member().build();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    thread.status = ferum_domain::models::thread::ThreadStatus::Deleted;

    let mut th = MockThreadRepository::new();
    th.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = build(MockBookmarkRepository::new(), th).add(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn add_already_bookmarked_returns_true_without_inserting() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let bookmark = make_bookmark(actor.id, ids::thread_a());
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut th = MockThreadRepository::new();
    th.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let mut bm = MockBookmarkRepository::new();
    bm.expect_find().return_once(move |_, _| Ok(Some(bookmark)));
    // expect_add NOT set — must not be called

    let result = build(bm, th).add(&actor, ids::thread_a()).await;
    assert!(result.unwrap());
}

#[tokio::test]
async fn add_new_bookmark_inserts_and_returns_true() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let bookmark = make_bookmark(actor.id, ids::thread_a());

    let mut th = MockThreadRepository::new();
    th.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let mut bm = MockBookmarkRepository::new();
    bm.expect_find().return_once(|_, _| Ok(None));
    bm.expect_add().return_once(move |_, _| Ok(bookmark));

    let result = build(bm, th).add(&actor, ids::thread_a()).await;
    assert!(result.unwrap());
}

// ─── remove ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn remove_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();
    let (bm, th) = (MockBookmarkRepository::new(), MockThreadRepository::new());
    let result = build(bm, th).remove(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn remove_succeeds_returns_false() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut bm = MockBookmarkRepository::new();
    bm.expect_remove().return_once(|_, _| Ok(()));

    let result = build(bm, MockThreadRepository::new()).remove(&actor, ids::thread_a()).await;
    assert!(!result.unwrap());
}

// ─── list ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();
    let (bm, th) = (MockBookmarkRepository::new(), MockThreadRepository::new());
    let result = build(bm, th).list(&actor, 1, 20).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn list_caps_per_page_at_50() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut bm = MockBookmarkRepository::new();
    // Verify per_page is capped: the mock expects exactly per_page=50, not 100
    bm.expect_list_for_user()
        .withf(|_, _, per_page| *per_page == 50)
        .return_once(|_, _, _| Ok((vec![], 0)));

    let result = build(bm, MockThreadRepository::new()).list(&actor, 1, 100).await;
    assert!(result.is_ok());
}
