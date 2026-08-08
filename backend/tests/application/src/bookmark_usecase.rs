use std::sync::Arc;

use ferum_application::usecases::bookmark_usecase::BookmarkUseCase;
use ferum_domain::AppError;
use ferum_test_support::fixtures::{ids, make_bookmark, make_category, make_thread, AuthUserBuilder};
use ferum_test_support::mocks::{
    bookmark_repository::MockBookmarkRepository,
    category_repository::MockCategoryRepository,
    thread_repository::MockThreadRepository,
};

// ─── Builder ──────────────────────────────────────────────────────────────────

/// Defaults the category to a public one, since that is the uninteresting case
/// for every test that is not about visibility. Use `build_with_category` to
/// vary it.
fn build(bookmarks: MockBookmarkRepository, threads: MockThreadRepository) -> BookmarkUseCase {
    let mut categories = MockCategoryRepository::new();
    categories
        .expect_find_by_id()
        .returning(move |_| Ok(Some(make_category(ids::category_a()))));
    BookmarkUseCase::new(Arc::new(bookmarks), Arc::new(threads), Arc::new(categories))
}

fn build_with_category(
    bookmarks: MockBookmarkRepository,
    threads: MockThreadRepository,
    categories: MockCategoryRepository,
) -> BookmarkUseCase {
    BookmarkUseCase::new(Arc::new(bookmarks), Arc::new(threads), Arc::new(categories))
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

// ─── category visibility ──────────────────────────────────────────────────────

/// Bookmarking never checked `view_policy`. The bookmarks page then renders the
/// thread's title, so a restricted title could reach a reader who was never
/// allowed to open the thread — the only path by which that could happen.
#[tokio::test]
async fn add_in_a_staff_only_category_is_not_found_for_an_outsider() {
    let actor = AuthUserBuilder::member().build();

    let mut th = MockThreadRepository::new();
    th.expect_find_by_id()
        .returning(move |_| Ok(Some(make_thread(ids::thread_a(), ids::category_a(), ids::user_b()))));

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(move |_| {
        let mut c = make_category(ids::category_a());
        c.view_policy = ferum_domain::models::category::ViewPolicy::StaffOnly;
        Ok(Some(c))
    });

    let mut bm = MockBookmarkRepository::new();
    bm.expect_add().never();

    let result = build_with_category(bm, th, cats).add(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)), "got {result:?}");
}

/// A moderator assigned to that category still can, so this narrows nothing for
/// the people the category is for.
#[tokio::test]
async fn add_in_a_staff_only_category_is_allowed_for_staff() {
    let actor = AuthUserBuilder::member()
        .with_category_perm(ids::category_a(), "moderation.view_reports")
        .build();

    let mut th = MockThreadRepository::new();
    th.expect_find_by_id()
        .returning(move |_| Ok(Some(make_thread(ids::thread_a(), ids::category_a(), ids::user_b()))));

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(move |_| {
        let mut c = make_category(ids::category_a());
        c.view_policy = ferum_domain::models::category::ViewPolicy::StaffOnly;
        Ok(Some(c))
    });

    let mut bm = MockBookmarkRepository::new();
    bm.expect_find().returning(|_, _| Ok(None));
    bm.expect_add()
        .times(1)
        .returning(move |_, _| Ok(make_bookmark(ids::user_a(), ids::thread_a())));

    build_with_category(bm, th, cats).add(&actor, ids::thread_a()).await.unwrap();
}
