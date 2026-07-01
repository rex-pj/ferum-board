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

// ─── delete_by_slug ───────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_by_slug_by_author_succeeds() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_slug().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(make_thread(ids::thread_a(), ids::category_a(), ids::user_a())));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_by_slug_by_moderator_with_cat_perm_succeeds() {
    let cat_id = ids::category_a();
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_category_perm(cat_id, "thread.delete_any")
        .build();
    let thread = make_thread(ids::thread_a(), cat_id, ids::user_b()); // different author

    let mut b = Uc::new();
    b.threads.expect_find_by_slug().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(make_thread(ids::thread_a(), cat_id, ids::user_b())));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_by_slug_non_author_without_perm_fails() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b()); // different author

    let mut b = Uc::new();
    b.threads.expect_find_by_slug().return_once(move |_| Ok(Some(thread)));

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn delete_by_slug_already_deleted_returns_not_found() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    thread.status = ThreadStatus::Deleted;

    let mut b = Uc::new();
    b.threads.expect_find_by_slug().return_once(move |_| Ok(Some(thread)));

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn delete_by_slug_thread_not_found() {
    let actor = AuthUserBuilder::admin().build();
    let mut b = Uc::new();
    b.threads.expect_find_by_slug().return_once(|_| Ok(None));

    let result = b.build().delete_by_slug(&actor, "nonexistent-thread").await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn delete_by_slug_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).banned().build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_slug().return_once(move |_| Ok(Some(thread)));

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
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

// Regression: two concurrent "mark best answer" calls for the *same* answer must
// not double-fire the event or double-award trust score to the post author.
#[tokio::test]
async fn mark_solved_already_solved_with_same_answer_is_idempotent_noop() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    thread.is_solved = true;
    thread.best_answer_id = Some(ids::post_a());
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    // No expect_update, expect_publish, or increment_trust_score set — the mock
    // panics if any of them are actually called, proving the no-op path is taken.

    let result = b.build().mark_solved(&actor, ids::thread_a(), ids::post_a()).await;
    assert!(result.is_ok());
}

// ─── update_tags ─────────────────────────────────────────────────────────────
// Regression coverage for the edit-thread handler bug where `tags` sent from the
// client were silently dropped because the handler never called this use case.

fn make_tag(id: uuid::Uuid, name: &str) -> ferum_domain::models::tag::Tag {
    ferum_domain::models::tag::Tag {
        id,
        name: name.to_string(),
        slug: slug::slugify(name),
        color: None,
        created_by_id: None,
        created_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn update_tags_author_within_window_replaces_thread_tags() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    let tag_id = uuid::Uuid::new_v4();
    let tag = make_tag(tag_id, "rust");

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.tags.expect_find_by_slug().return_once(move |_| Ok(Some(tag)));
    b.tags
        .expect_replace_thread_tags()
        .withf(move |thread_id, tag_ids| *thread_id == ids::thread_a() && tag_ids == [tag_id])
        .return_once(|_, _| Ok(()));

    let result = b.build().update_tags(&actor, ids::thread_a(), vec!["rust".to_string()]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn update_tags_unknown_tag_without_create_perm_is_skipped() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build(); // no tag.create perm
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.tags.expect_find_by_slug().return_once(|_| Ok(None));
    // No create() expectation — must not attempt to create a tag without permission.
    b.tags
        .expect_replace_thread_tags()
        .withf(|_, tag_ids: &[uuid::Uuid]| tag_ids.is_empty())
        .return_once(|_, _| Ok(()));

    let result = b.build().update_tags(&actor, ids::thread_a(), vec!["brand-new-tag".to_string()]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn update_tags_locked_thread_without_edit_any_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    thread.status = ThreadStatus::Locked;

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().update_tags(&actor, ids::thread_a(), vec!["rust".to_string()]).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "thread_locked"));
}

#[tokio::test]
async fn update_tags_non_author_without_edit_any_perm_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b()); // different author

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().update_tags(&actor, ids::thread_a(), vec!["rust".to_string()]).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "edit_window_expired"));
}
