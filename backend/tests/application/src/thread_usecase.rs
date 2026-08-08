use std::sync::Arc;

use ferum_application::usecases::thread_usecase::ThreadUseCase;
use ferum_domain::AppError;
use ferum_domain::models::thread::ThreadStatus;
use ferum_domain::repositories::stored_file_repository::{StoredFileRepository, UploadUsage};
use ferum_test_support::fixtures::{ids, make_category, make_post, make_thread, AuthUserBuilder};
use ferum_test_support::mocks::{
    cache_service::NoopCacheService,
    category_repository::MockCategoryRepository,
    event_publisher::MockEventPublisher,
    job_queue::NoopJobQueue,
    post_repository::MockPostRepository,
    storage_service::NoopStorageService,
    stored_file_repository::NoopStoredFileRepository,
    tag_repository::MockTagRepository,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
};

// ─── Builder ──────────────────────────────────────────────────────────────────

/// Records which CAS keys were dereferenced, so a test can assert that deleting
/// a thread actually releases the attachments its posts held.
#[derive(Default)]
struct SpyStoredFiles {
    dereferenced: std::sync::Mutex<Vec<String>>,
}

impl SpyStoredFiles {
    fn dereferenced_sorted(&self) -> Vec<String> {
        let mut v = self.dereferenced.lock().unwrap().clone();
        v.sort();
        v
    }
}

#[async_trait::async_trait]
impl StoredFileRepository for SpyStoredFiles {
    async fn usage_since(&self, _: uuid::Uuid, _: chrono::DateTime<chrono::Utc>) -> Result<UploadUsage, AppError> {
        Ok(UploadUsage { file_count: 0, total_bytes: 0 })
    }
    async fn upsert_and_ref(&self, _: &str, _: &str, _: i64, _: Option<uuid::Uuid>) -> Result<(), AppError> { Ok(()) }
    async fn upsert_staged(&self, _: &str, _: &str, _: i64, _: Option<uuid::Uuid>) -> Result<(), AppError> { Ok(()) }
    async fn increment_ref(&self, _: &str) -> Result<(), AppError> { Ok(()) }
    async fn read_data(&self, _: &str) -> Result<Option<(Vec<u8>, String)>, AppError> { Ok(None) }
    async fn clear_data(&self, _: &str) -> Result<(), AppError> { Ok(()) }
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError> {
        self.dereferenced.lock().unwrap().push(key.to_string());
        Ok(0)
    }
    async fn delete_by_key(&self, _: &str) -> Result<(), AppError> { Ok(()) }
    async fn delete_if_unreferenced(&self, _: &str) -> Result<bool, AppError> { Ok(true) }
    async fn list_keys_with_prefix(&self, _: &str) -> Result<Vec<String>, AppError> { Ok(vec![]) }
}

struct Uc {
    threads: MockThreadRepository,
    categories: MockCategoryRepository,
    posts: MockPostRepository,
    tags: MockTagRepository,
    users: MockUserRepository,
    events: MockEventPublisher,
    stored_files: Arc<dyn StoredFileRepository>,
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
            stored_files: Arc::new(NoopStoredFileRepository),
        }
    }

    fn build(self) -> ThreadUseCase {
        ThreadUseCase::new(
            Arc::new(self.threads),
            Arc::new(self.categories),
            Arc::new(self.posts),
            Arc::new(NoopJobQueue),
            self.stored_files,
            Arc::new(NoopStorageService),
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
    b.posts.expect_content_md_by_thread().return_once(|_| Ok(vec![]));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_by_slug_releases_attachment_refs_of_every_live_post() {
    const A: &str = "post-attachments/0123456789abcdef0123456789abcdef.png";
    const B: &str = "post-attachments/fedcba9876543210fedcba9876543210.webp";

    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let spy = Arc::new(SpyStoredFiles::default());
    let mut b = Uc::new();
    b.stored_files = spy.clone();
    b.threads.expect_find_by_slug().return_once(move |_| Ok(Some(thread)));
    b.threads.expect_update().return_once(move |_, _| Ok(make_thread(ids::thread_a(), ids::category_a(), ids::user_a())));
    b.events.expect_publish().return_once(|_| ());
    b.posts.expect_content_md_by_thread().return_once(move |_| {
        Ok(vec![
            // Embeds A twice: one post holds exactly one reference to it.
            format!("![x](/files/{A}) again ![x](/files/{A})"),
            // A second post independently embeds A — that is a second reference.
            format!("![x](/files/{A}) and ![y](/files/{B})"),
            "no attachments here".to_string(),
        ])
    });

    let result = b.build().delete_by_slug(&actor, "test-thread").await;
    assert!(result.is_ok());

    // A was referenced once per post (2 posts) => released twice. B once.
    assert_eq!(
        spy.dereferenced_sorted(),
        vec![A.to_string(), A.to_string(), B.to_string()],
        "each post must give back exactly the references it took"
    );
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
    b.posts.expect_content_md_by_thread().return_once(|_| Ok(vec![]));
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
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "best_answer_wrong_thread"),
        "expected best_answer_wrong_thread, got {result:?}");
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

/// Refused, not silently skipped.
///
/// This used to assert the opposite — that the unknown tag was dropped and the
/// call still reported success. That is what the code did, and it is the wrong
/// behaviour: `ThreadUseCase::create` returns `tag_create_permission_required`
/// for exactly this situation, so the same user adding the same tag got an
/// error when starting a thread and silent success when editing one. The chips
/// simply vanished on reload with nothing having said why.
#[tokio::test]
async fn update_tags_unknown_tag_without_create_perm_is_refused() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build(); // no tag.create perm
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.tags.expect_find_by_slug().return_once(|_| Ok(None));
    // Must neither create the tag nor write a silently-emptied tag set.
    b.tags.expect_create().never();
    b.tags.expect_replace_thread_tags().never();

    let result = b.build().update_tags(&actor, ids::thread_a(), vec!["brand-new-tag".to_string()]).await;
    assert!(
        matches!(&result, Err(AppError::Forbidden(c)) if c == "tag_create_permission_required"),
        "got {result:?}"
    );
}

/// Removing every tag is still allowed — an empty *requested* set is a real
/// edit, unlike an empty set that is the residue of dropped tags.
#[tokio::test]
async fn update_tags_can_clear_all_tags() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);

    let mut b = Uc::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.tags
        .expect_replace_thread_tags()
        .withf(|_, tag_ids: &[uuid::Uuid]| tag_ids.is_empty())
        .times(1)
        .return_once(|_, _| Ok(()));

    b.build().update_tags(&actor, ids::thread_a(), vec![]).await.unwrap();
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

/// Opening a thread needs `thread.create`, not just `post.create`.
///
/// The two were one gate for a long time: `create` called `can_create_post`,
/// so `thread.create` was seeded and grantable while nothing read it, and
/// revoking it in /admin/permissions did nothing. This pins the separation —
/// a member allowed to reply is not thereby allowed to start a topic.
#[tokio::test]
async fn create_requires_thread_create_permission() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perm("post.create") // deliberately WITHOUT thread.create
        .build();

    let mut b = Uc::new();
    b.categories
        .expect_find_by_id()
        .return_once(|_| Ok(Some(make_category(ids::category_a()))));

    let result = b.build().create(&actor, review_cmd(None)).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "permission_denied"));
}

// ─── create: one review per author per product ───────────────────────────────
//
// `uq_threads_product_author` (migration 28) is the real guarantee; these cover
// the use-case check that turns a would-be unique violation into a 409 the
// caller can act on. Without the rule one account can open N threads on the
// same product and set the product's average single-handed, because
// recompute_stats sums every rating on every non-deleted thread and does not
// group by author.

fn review_cmd(product_id: Option<uuid::Uuid>) -> ferum_application::usecases::thread_usecase::CreateThreadCmd {
    ferum_application::usecases::thread_usecase::CreateThreadCmd {
        category_id: ids::category_a(),
        title: "Ghế công thái học dùng 6 tháng".to_string(),
        content_md: "Chi tiết cảm nhận sau nửa năm.".to_string(),
        tag_names: vec![],
        product_id,
    }
}

#[tokio::test]
async fn create_rejects_second_review_of_same_product() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.create", "thread.create"])
        .build();
    let existing = make_thread(ids::thread_a(), ids::category_a(), ids::user_a());

    let mut b = Uc::new();
    b.categories.expect_find_by_id().return_once(|_| Ok(Some(make_category(ids::category_a()))));
    b.threads.expect_find_review_by_author().return_once(move |_, _| Ok(Some(existing)));
    // expect_create NOT set — the thread must never be inserted.

    let result = b.build().create(&actor, review_cmd(Some(ids::product_a()))).await;
    assert!(matches!(result, Err(AppError::Conflict(c)) if c == "product_already_reviewed"));
}

#[tokio::test]
async fn create_allows_first_review_of_product() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.create", "thread.create"])
        .build();
    let created = make_thread(ids::thread_a(), ids::category_a(), ids::user_a());

    let mut b = Uc::new();
    b.categories.expect_find_by_id().return_once(|_| Ok(Some(make_category(ids::category_a()))));
    b.threads.expect_find_review_by_author().return_once(|_, _| Ok(None));
    b.threads.expect_create().return_once(move |_| Ok(created));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().create(&actor, review_cmd(Some(ids::product_a()))).await;
    assert!(result.is_ok());
}

// ─── list_feed: reviews excluded from the discussion feed ────────────────────
//
// Reviews are threads but not discussion, so the homepage feed must not list the
// canonical reviews category. These assert on the exact category-id set handed to
// `list_feed` — the whole behaviour lives in which ids reach the repository.

/// A category with an explicit id and slug (the shared fixture hard-codes slug
/// "general"; these tests need the "reviews" slug to be recognised).
fn category_with_slug(id: uuid::Uuid, slug: &str) -> ferum_domain::models::category::Category {
    ferum_domain::models::category::Category {
        slug: slug.to_string(),
        ..make_category(id)
    }
}

fn reviews_category_id() -> uuid::Uuid {
    uuid::Uuid::parse_str("00000000-0000-0000-0000-0000000000e5").unwrap()
}

#[tokio::test]
async fn list_feed_excludes_reviews_category_for_guest() {
    use ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG;
    let general = ids::category_a();
    let reviews = reviews_category_id();

    let mut b = Uc::new();
    b.categories.expect_list_all().returning(move || {
        Ok(vec![
            category_with_slug(general, "general"),
            category_with_slug(reviews, REVIEWS_CATEGORY_SLUG),
        ])
    });
    // The assertion: the reviews id must never be among the categories queried.
    b.threads
        .expect_list_feed()
        .withf(move |ids, _, _, _, _| ids.contains(&general) && !ids.contains(&reviews))
        .return_once(|_, _, _, _, _| Ok((vec![], 0)));
    b.tags.expect_find_by_threads().return_once(|_| Ok(Default::default()));

    let result = b
        .build()
        .list_feed(None, ferum_domain::repositories::thread_repository::ThreadSort::Activity, ferum_domain::repositories::thread_repository::ThreadFeedFilter::All, 1, 20)
        .await;
    assert!(result.is_ok(), "guest feed should build without the reviews category");
}

#[tokio::test]
async fn list_feed_keeps_reviews_when_user_explicitly_watches_it() {
    use ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG;
    let general = ids::category_a();
    let reviews = reviews_category_id();
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut b = Uc::new();
    b.categories.expect_list_all().returning(move || {
        Ok(vec![
            category_with_slug(general, "general"),
            category_with_slug(reviews, REVIEWS_CATEGORY_SLUG),
        ])
    });
    // The user has deliberately watched the reviews category — that opt-in must
    // win over the blanket exclusion, otherwise the toggle does nothing.
    b.users
        .expect_get_watched_categories()
        .return_once(move |_| Ok(vec![reviews]));
    b.users
        .expect_get_muted_categories()
        .return_once(|_| Ok(vec![]));
    b.threads
        .expect_list_feed()
        .withf(move |ids, _, _, _, _| ids.contains(&reviews))
        .return_once(|_, _, _, _, _| Ok((vec![], 0)));
    b.tags.expect_find_by_threads().return_once(|_| Ok(Default::default()));

    let result = b
        .build()
        .list_feed(Some(&actor), ferum_domain::repositories::thread_repository::ThreadSort::Activity, ferum_domain::repositories::thread_repository::ThreadFeedFilter::All, 1, 20)
        .await;
    assert!(result.is_ok(), "an explicit watch on reviews must survive the exclusion");
}

#[tokio::test]
async fn create_skips_review_check_for_non_product_thread() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.create", "thread.create"])
        .build();
    let created = make_thread(ids::thread_a(), ids::category_a(), ids::user_a());

    let mut b = Uc::new();
    b.categories.expect_find_by_id().return_once(|_| Ok(Some(make_category(ids::category_a()))));
    // expect_find_review_by_author NOT set — an ordinary thread must not pay for
    // the lookup, and mockall fails the test if it is called anyway.
    b.threads.expect_create().return_once(move |_| Ok(created));
    b.events.expect_publish().return_once(|_| ());

    let result = b.build().create(&actor, review_cmd(None)).await;
    assert!(result.is_ok());
}
