use std::sync::Arc;

use uuid::Uuid;
use mockall::predicate::eq;
use ferum_application::ports::NullPluginRuntime;
use ferum_application::usecases::post_usecase::{CreatePostCmd, PostUseCase};
use ferum_domain::AppError;
use ferum_domain::models::category::PostPolicy;
use ferum_domain::models::thread::ThreadStatus;
use ferum_test_support::fixtures::{ids, make_category, make_post, make_thread, AuthUserBuilder};
use ferum_test_support::mocks::{
    category_repository::MockCategoryRepository,
    event_publisher::MockEventPublisher,
    post_repository::MockPostRepository,
    reaction_repository::MockReactionRepository,
    site_config::InMemorySiteConfig,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
};

// ─── Test builder ─────────────────────────────────────────────────────────────

struct PostUseCaseBuilder {
    posts: MockPostRepository,
    threads: MockThreadRepository,
    categories: MockCategoryRepository,
    users: MockUserRepository,
    reactions: MockReactionRepository,
    site_config: InMemorySiteConfig,
    events: MockEventPublisher,
}

impl PostUseCaseBuilder {
    fn new() -> Self {
        Self {
            posts: MockPostRepository::new(),
            threads: MockThreadRepository::new(),
            categories: MockCategoryRepository::new(),
            users: MockUserRepository::new(),
            reactions: MockReactionRepository::new(),
            site_config: InMemorySiteConfig::empty(),
            events: MockEventPublisher::new(),
        }
    }

    fn build(self) -> PostUseCase {
        PostUseCase::new(
            Arc::new(self.posts),
            Arc::new(self.threads),
            Arc::new(self.categories),
            Arc::new(self.users),
            Arc::new(self.reactions),
            Arc::new(self.site_config),
            Arc::new(self.events),
        )
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn cmd(thread_id: Uuid) -> CreatePostCmd {
    CreatePostCmd {
        thread_id,
        parent_id: None,
        content_md: "Hello world".to_string(),
    }
}

fn member_with_post_perm() -> ferum_domain::AuthUser {
    AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.create", "post.edit_own", "post.delete_own"])
        .build()
}

// ─── create: happy path ───────────────────────────────────────────────────────

#[tokio::test]
async fn create_post_happy_path() {
    let thread_id = ids::thread_a();
    let cat_id = ids::category_a();
    let actor = member_with_post_perm();
    let thread = make_thread(thread_id, cat_id, ids::user_b());
    let category = make_category(cat_id);
    let post = make_post(ids::post_a(), thread_id, actor.id);

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().with(eq(thread_id)).return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().with(eq(cat_id)).return_once(move |_| Ok(Some(category)));
    b.posts.expect_create().return_once(move |_| Ok(post));
    b.threads.expect_update_reply_stats().return_once(|_, _, _| Ok(()));
    b.users.expect_increment_post_count().return_once(|_, _| Ok(()));
    b.events.expect_publish().return_once(|_| ());

    let uc = b.build();
    let result = uc.create(&actor, cmd(thread_id)).await;
    assert!(result.is_ok());
}

// ─── create: guard checks ─────────────────────────────────────────────────────

#[tokio::test]
async fn create_post_thread_not_found() {
    let actor = member_with_post_perm();
    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(|_| Ok(None));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn create_post_thread_deleted_returns_not_found() {
    let actor = member_with_post_perm();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    thread.deleted_at = Some(chrono::Utc::now());

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn create_post_thread_locked_returns_forbidden() {
    let actor = member_with_post_perm();
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    thread.status = ThreadStatus::Locked;

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_post_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().with_perms(&["post.create"]).build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let category = make_category(ids::category_a());

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(category)));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_post_no_permission_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build(); // no post.create
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let category = make_category(ids::category_a());

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(category)));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_post_closed_category_returns_forbidden() {
    let actor = member_with_post_perm();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let mut category = make_category(ids::category_a());
    category.post_policy = PostPolicy::Closed;

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(category)));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_post_trusted_category_with_basic_trust_returns_forbidden() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_trust(ferum_domain::models::user::TrustLevel::Basic)
        .with_perms(&["post.create"])
        .build();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let mut category = make_category(ids::category_a());
    category.post_policy = PostPolicy::Trusted; // requires Member trust

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(category)));

    let result = b.build().create(&actor, cmd(ids::thread_a())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// ─── create: content validation ───────────────────────────────────────────────

#[tokio::test]
async fn create_post_empty_content_returns_unprocessable() {
    let actor = member_with_post_perm();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let category = make_category(ids::category_a());

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(category)));

    let result = b.build().create(&actor, CreatePostCmd {
        thread_id: ids::thread_a(),
        parent_id: None,
        content_md: "   ".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn create_post_content_too_large_returns_unprocessable() {
    let actor = member_with_post_perm();
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    let category = make_category(ids::category_a());

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().return_once(move |_| Ok(Some(category)));

    let huge = "x".repeat(102_401); // > 100 KB
    let result = b.build().create(&actor, CreatePostCmd {
        thread_id: ids::thread_a(),
        parent_id: None,
        content_md: huge,
    }).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

// ─── delete ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_own_published_post_succeeds() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.delete_own"])
        .build();
    let post = make_post(ids::post_a(), ids::thread_a(), actor.id);
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_soft_delete().return_once(|_, _| Ok(()));
    b.events.expect_publish().return_once(|_| ());
    b.threads.expect_update_reply_stats().return_once(|_, _, _| Ok(()));
    b.users.expect_increment_post_count().return_once(|_, _| Ok(()));

    let result = b.build().delete(&actor, ids::post_a()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_post_not_found() {
    let actor = member_with_post_perm();
    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(|_| Ok(None));

    let result = b.build().delete(&actor, ids::post_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn delete_already_deleted_post_returns_not_found() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.delete_own"])
        .build();
    let mut post = make_post(ids::post_a(), ids::thread_a(), actor.id);
    post.is_deleted = true;
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().delete(&actor, ids::post_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn delete_other_users_post_without_permission_returns_forbidden() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.delete_own"])
        .build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b()); // different author
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().delete(&actor, ids::post_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn delete_pending_post_does_not_update_reply_stats() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.delete_own"])
        .build();
    let mut post = make_post(ids::post_a(), ids::thread_a(), actor.id);
    post.status = ferum_domain::models::post::PostStatus::Pending;
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_soft_delete().return_once(|_, _| Ok(()));
    b.events.expect_publish().return_once(|_| ());
    // update_reply_stats and increment_post_count must NOT be called

    let result = b.build().delete(&actor, ids::post_a()).await;
    assert!(result.is_ok());
}

// ─── edit ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn edit_empty_content_returns_unprocessable() {
    let actor = member_with_post_perm();
    let b = PostUseCaseBuilder::new();
    let result = b.build().edit(&actor, ids::post_a(), "  ".to_string()).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn edit_post_not_found() {
    let actor = member_with_post_perm();
    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(|_| Ok(None));

    let result = b.build().edit(&actor, ids::post_a(), "new content".to_string()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn edit_other_users_post_without_permission_returns_forbidden() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["post.edit_own"])
        .build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b()); // different author
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));

    let result = b.build().edit(&actor, ids::post_a(), "new content".to_string()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}
