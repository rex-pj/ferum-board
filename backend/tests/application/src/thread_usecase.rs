use std::sync::Arc;

use ferum_application::usecases::thread_usecase::ThreadUseCase;
use ferum_domain::AppError;
use ferum_domain::models::thread::ThreadStatus;
use ferum_test_support::fixtures::{ids, make_category, make_post, make_thread, AuthUserBuilder};
use ferum_test_support::mocks::{
    cache_service::NoopCacheService,
    category_repository::MockCategoryRepository,
    event_publisher::MockEventPublisher,
    job_queue::NoopJobQueue,
    post_repository::MockPostRepository,
    stored_file_repository::NoopStoredFileRepository,
    tag_repository::MockTagRepository,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
};

// ─── Builder ──────────────────────────────────────────────────────────────────

struct Uc {
    threads: MockThreadRepository,
    categories: MockCategoryRepository,
    posts: MockPostRepository,
    tags: MockTagRepository,
    users: MockUserRepository,
    events: MockEventPublisher,
}

impl Uc {
    fn new() -> Self {
        Self {
            threads: MockThreadRepository::new(),
            categories: MockCategoryRepository::new(),
            posts: MockPostRepository::new(),
            tags: MockTagRepository::new(),
            users: MockUserRepository::new(),
            events: MockEventPublisher::new(),
        }
    }

    fn build(self) -> ThreadUseCase {
        ThreadUseCase::new(
            Arc::new(self.threads),
            Arc::new(self.categories),
            Arc::new(self.posts),
            Arc::new(NoopJobQueue),
            Arc::new(NoopStoredFileRepository),
            Arc::new(self.events),
            Arc::new(NoopCacheService),
            Arc::new(self.tags),
            Arc::new(self.users),
        )
    }
}

// ─── pin ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_succeeds_with_category_scoped_perm() {
    let cat_id = ids::category_a();
    let actor = AuthUserBuilder::member()
        .with_category_perm(cat_id, "thread.pin")
        .build();
    let thread = make_thread(ids::thread_a(), cat_id, ids::user_b());
    let updated = make_thread(ids::thread_a(), cat_id, ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(updated));

    let result = b.build().pin(&actor, ids::thread_a(), true).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn pin_fails_without_permission() {
    let actor = AuthUserBuilder::member().build(); // no thread.pin
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().pin(&actor, ids::thread_a(), true).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn pin_fails_when_global_perm_but_not_in_this_category() {
    // has thread.pin in category_b but NOT in category_a
    let other_cat = ids::user_b(); // reuse a UUID as "other category"
    let actor = AuthUserBuilder::member()
        .with_category_perm(other_cat, "thread.pin")
        .build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().pin(&actor, ids::thread_a(), true).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn pin_thread_not_found() {
    let actor = AuthUserBuilder::admin().build();
    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(|_| Ok(None));

    let result = b.build().pin(&actor, ids::thread_a(), true).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

// ─── lock ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn lock_publishes_event_when_locking() {
    let cat_id = ids::category_a();
    let actor = AuthUserBuilder::member()
        .with_category_perm(cat_id, "thread.lock")
        .build();
    let thread = make_thread(ids::thread_a(), cat_id, ids::user_b());
    let updated = make_thread(ids::thread_a(), cat_id, ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(updated));
    b.events.expect_publish().return_once(|_| ()); // event must be published

    let result = b.build().lock(&actor, ids::thread_a(), true).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn unlock_does_not_publish_event() {
    let cat_id = ids::category_a();
    let actor = AuthUserBuilder::member()
        .with_category_perm(cat_id, "thread.lock")
        .build();
    let thread = make_thread(ids::thread_a(), cat_id, ids::user_b());
    let updated = make_thread(ids::thread_a(), cat_id, ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(updated));
    // expect_publish NOT set — mockall will fail if it is called unexpectedly

    let result = b.build().lock(&actor, ids::thread_a(), false).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn lock_fails_without_permission() {
    let actor = AuthUserBuilder::member().build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().lock(&actor, ids::thread_a(), true).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// ─── move_to ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn move_succeeds_with_category_scoped_perm() {
    let src_cat = ids::category_a();
    let dst_cat = ids::user_b(); // reuse UUID as second category
    let actor = AuthUserBuilder::member()
        .with_category_perm(src_cat, "thread.move")
        .build();
    let thread = make_thread(ids::thread_a(), src_cat, ids::user_a());
    let dst_category = make_category(dst_cat);
    let updated = make_thread(ids::thread_a(), dst_cat, ids::user_a());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(dst_category)));
    b.threads.expect_update().return_once(move |_, _| Ok(updated));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().move_to(&actor, ids::thread_a(), dst_cat).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn move_fails_without_permission() {
    let actor = AuthUserBuilder::member().build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().move_to(&actor, ids::thread_a(), ids::user_b()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn move_target_category_not_found() {
    let cat_id = ids::category_a();
    let actor = AuthUserBuilder::member()
        .with_category_perm(cat_id, "thread.move")
        .build();
    let thread = make_thread(ids::thread_a(), cat_id, ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(|_| Ok(None));

    let result = b.build().move_to(&actor, ids::thread_a(), ids::user_b()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

// ─── soft_delete ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn soft_delete_by_author_succeeds() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    let updated = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(updated));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().soft_delete(&actor, ids::thread_a()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn soft_delete_by_non_author_without_perm_fails() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b()); // different author

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().soft_delete(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn soft_delete_already_deleted_returns_not_found() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    thread.deleted_at = Some(chrono::Utc::now());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().soft_delete(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn soft_delete_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).banned().build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().soft_delete(&actor, ids::thread_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// ─── mark_solved ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn mark_solved_by_author_succeeds() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    let updated = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_update().return_once(move |_, _| Ok(updated));
    b.events.expect_publish().return_once(|_| ());
    b.users.expect_increment_trust_score().return_once(|_, _| Ok(()));

    let result = b.build().mark_solved(&actor, ids::thread_a(), ids::post_a()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn mark_solved_best_post_from_different_thread_returns_unprocessable() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    let mut post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    post.thread_id = ids::user_b(); // wrong thread_id

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));

    let result = b.build().mark_solved(&actor, ids::thread_a(), ids::post_a()).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn mark_solved_non_author_needs_lock_perm() {
    let cat_id = ids::category_a();
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build(); // not author, no perm
    let thread = make_thread(ids::thread_a(), cat_id, ids::user_b()); // different author

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().mark_solved(&actor, ids::thread_a(), ids::post_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}
