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
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "post_content_empty"),
        "expected post_content_empty, got {result:?}");
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
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "post_content_too_long"),
        "expected post_content_too_long, got {result:?}");
}

// ─── approval queue ───────────────────────────────────────────────────────────

/// `reject_post` soft-deletes but leaves `status = Pending`, so a rejected post
/// stays pending forever. `approve_post` only checked `is_pending()`, so calling
/// it on an already-rejected post published a row that can never render — and
/// incremented `threads.reply_count` and the author's `post_count` for it,
/// permanently, with nothing to reconcile them against.
///
/// The queue UI hides rejected posts (`list_pending` filters `is_deleted`), but
/// `POST /api/mod/queue/:id/approve` is reachable directly — a stale tab, a
/// double-click, or two moderators working the queue at once.
#[tokio::test]
async fn approving_an_already_rejected_post_is_refused() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["moderation.view_reports"])
        .build();

    let mut post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    post.status = ferum_domain::models::post::PostStatus::Pending;
    post.is_deleted = true; // rejected earlier

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    // The counters are the damage: neither may move.
    b.posts.expect_set_status().never();
    b.threads.expect_update_reply_stats().never();
    b.users.expect_increment_post_count().never();

    let result = b.build().approve_post(&actor, ids::post_a()).await;
    assert!(matches!(result, Err(AppError::NotFound)), "got {result:?}");
}

/// The ordinary path still works — this is a guard on the deleted case, not a
/// narrowing of approval.
#[tokio::test]
async fn approving_a_live_pending_post_publishes_it() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["moderation.view_reports"])
        .build();

    let mut post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    post.status = ferum_domain::models::post::PostStatus::Pending;
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_set_status().times(1).returning(|_, _| Ok(()));
    b.threads.expect_update_reply_stats().times(1).returning(|_, _, _| Ok(()));
    b.users.expect_increment_post_count().times(1).returning(|_, _| Ok(()));
    b.users.expect_find_by_id().returning(|_| Ok(None));
    b.events.expect_publish().returning(|_| ());

    b.build().approve_post(&actor, ids::post_a()).await.unwrap();
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
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "post_content_empty"),
        "expected post_content_empty, got {result:?}");
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

/// Locking a thread means the conversation is closed. `create` refuses, and
/// `ThreadUseCase::update_title` refuses — but `can_edit_post` never saw the
/// thread's status, so the author could still rewrite the body of any post in
/// it. Locking a thread over its content left that content editable.
#[tokio::test]
async fn edit_in_a_locked_thread_is_refused_for_the_author() {
    let actor = member_with_post_perm();
    let post = make_post(ids::post_a(), ids::thread_a(), actor.id);
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    thread.status = ThreadStatus::Locked;

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_update_content().never();

    let result = b.build().edit(&actor, ids::post_a(), "rewritten".to_string()).await;
    assert!(matches!(&result, Err(AppError::Forbidden(c)) if c == "thread_locked"),
        "expected thread_locked, got {result:?}");
}

/// A lock is a moderation action, and `thread.edit_any` is the permission that
/// overrides it everywhere else (`update_title` already works this way). A
/// moderator must still be able to redact a post in a thread they just locked —
/// which is often exactly why they locked it.
#[tokio::test]
async fn edit_in_a_locked_thread_is_allowed_with_edit_any() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["thread.edit_any"])
        .build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());
    thread.status = ThreadStatus::Locked;
    let updated = make_post(ids::post_a(), ids::thread_a(), ids::user_b());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_update_content().return_once(move |_, _, _, _| Ok(updated));

    b.build().edit(&actor, ids::post_a(), "redacted".to_string()).await.unwrap();
}

/// A deleted thread is gone as far as readers are concerned, so its posts are
/// not editable either — `find_live_thread` is the rule everywhere else, and
/// `edit` was reading the thread without applying it.
#[tokio::test]
async fn edit_in_a_deleted_thread_is_refused() {
    let actor = member_with_post_perm();
    let post = make_post(ids::post_a(), ids::thread_a(), actor.id);
    let mut thread = make_thread(ids::thread_a(), ids::category_a(), actor.id);
    thread.status = ThreadStatus::Deleted;
    thread.deleted_at = Some(chrono::Utc::now());

    let mut b = PostUseCaseBuilder::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.posts.expect_update_content().never();

    let result = b.build().edit(&actor, ids::post_a(), "rewritten".to_string()).await;
    assert!(matches!(result, Err(AppError::NotFound)), "got {result:?}");
}

// ─── extract_attachment_keys ──────────────────────────────────────────────────

mod attachment_keys {
    use ferum_application::usecases::post_usecase::extract_attachment_keys;

    /// 32 lowercase hex chars — exactly what `cas_key` emits (16 bytes of SHA-256).
    const H1: &str = "0123456789abcdef0123456789abcdef";
    const H2: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn extracts_key_from_markdown_image() {
        let md = format!("hello\n\n![alt text](/files/post-attachments/{H1}.png)\n");
        let keys = extract_attachment_keys(&md);
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&format!("post-attachments/{H1}.png")));
    }

    #[test]
    fn deduplicates_the_same_image_embedded_twice() {
        // One post embedding an image twice must hold exactly one reference,
        // otherwise deleting the post leaves the count permanently above zero.
        let md = format!(
            "![a](/files/post-attachments/{H1}.png) and again ![b](/files/post-attachments/{H1}.png)"
        );
        assert_eq!(extract_attachment_keys(&md).len(), 1);
    }

    #[test]
    fn extracts_several_distinct_keys() {
        let md = format!(
            "![a](/files/post-attachments/{H1}.png)\n![b](/files/post-attachments/{H2}.webp)"
        );
        let keys = extract_attachment_keys(&md);
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&format!("post-attachments/{H2}.webp")));
    }

    #[test]
    fn ignores_other_cas_namespaces() {
        // Refs are only ever taken on the attachment namespace; avatars and
        // thumbnails are ref-counted by their own owning rows.
        let md = format!(
            "![a](/files/avatars/{H1}.png) ![b](/files/thumbnails/{H1}.png) ![c](/files/logos/{H1}.png)"
        );
        assert!(extract_attachment_keys(&md).is_empty());
    }

    #[test]
    fn rejects_keys_that_cas_key_could_never_have_produced() {
        // A crafted URL in post content must not become a lookup for anything
        // outside the attachment namespace's exact shape.
        let cases = [
            format!("/files/post-attachments/{}.png", &H1[..31]), // too short
            format!("/files/post-attachments/{H1}0.png"),         // too long
            format!("/files/post-attachments/{}.png", H1.to_uppercase()), // not lowercase hex
            format!("/files/post-attachments/{H1}.exe"),          // ext too long / not an image
            format!("/files/post-attachments/{H1}"),              // no extension
            "/files/post-attachments/../../etc/passwd".to_string(),
        ];
        for case in cases {
            assert!(
                extract_attachment_keys(&case).is_empty(),
                "must not extract a key from {case}"
            );
        }
    }

    #[test]
    fn empty_content_yields_no_keys() {
        assert!(extract_attachment_keys("").is_empty());
    }
}

// ─── Attachment promotion ─────────────────────────────────────────────────────
//
// Staged bytes stay in the database: at `ref_count == 0` the file is authorized
// per viewer, which is unenforceable once the object is world-readable.
//
// Driven through `create`, not by calling `promote_attachment` directly, so
// these also pin that promotion happens on the real publish path after
// `increment_ref` — and it stays private, as a handler must not call it.

use std::sync::Mutex;

use bytes::Bytes;
use ferum_application::ports::StorageService;
use ferum_domain::repositories::stored_file_repository::{StoredFileRepository, UploadUsage};

const ATTACHMENT_KEY: &str = "post-attachments/0123456789abcdef0123456789abcdef.jpg";

/// Ordered record of every storage/repository call, shared by both spies.
///
/// Order is the point: `put` must precede `clear_data`, because until the object
/// store has the bytes the row holds the only copy. A test that merely asserted
/// "both happened" would pass against the data-losing implementation.
type Journal = Arc<Mutex<Vec<&'static str>>>;

struct SpyStoredFiles {
    journal: Journal,
    /// `None` models a row whose bytes are already in the object store.
    data: Option<Vec<u8>>,
    increment_fails: bool,
}

#[async_trait::async_trait]
impl StoredFileRepository for SpyStoredFiles {
    async fn usage_since(
        &self,
        _: Uuid,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<UploadUsage, AppError> {
        Ok(UploadUsage { file_count: 0, total_bytes: 0 })
    }
    async fn upsert_and_ref(&self, _: &str, _: &str, _: i64, _: Option<Uuid>) -> Result<(), AppError> {
        Ok(())
    }
    async fn upsert_staged(&self, _: &str, _: &str, _: i64, _: Option<Uuid>) -> Result<(), AppError> {
        Ok(())
    }
    async fn increment_ref(&self, _: &str) -> Result<(), AppError> {
        if self.increment_fails {
            return Err(AppError::internal("increment failed"));
        }
        self.journal.lock().unwrap().push("increment_ref");
        Ok(())
    }
    async fn decrement_ref(&self, _: &str) -> Result<i32, AppError> {
        Ok(0)
    }
    async fn read_data(&self, _: &str) -> Result<Option<(Vec<u8>, String)>, AppError> {
        self.journal.lock().unwrap().push("read_data");
        Ok(self.data.clone().map(|d| (d, "image/jpeg".to_string())))
    }
    async fn clear_data(&self, _: &str) -> Result<(), AppError> {
        self.journal.lock().unwrap().push("clear_data");
        Ok(())
    }
    async fn delete_by_key(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete_if_unreferenced(&self, _: &str) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn list_keys_with_prefix(&self, _: &str) -> Result<Vec<String>, AppError> {
        Ok(vec![])
    }
}

struct SpyStorage {
    journal: Journal,
    /// Absolute = an object store is configured. Relative = database storage,
    /// where staging and serving are the same place and there is nothing to move.
    absolute_urls: bool,
    put_fails: bool,
}

#[async_trait::async_trait]
impl StorageService for SpyStorage {
    async fn put(&self, _: &str, _: Bytes, _: &str) -> Result<(), AppError> {
        if self.put_fails {
            self.journal.lock().unwrap().push("put_failed");
            return Err(AppError::internal("object store unavailable"));
        }
        self.journal.lock().unwrap().push("put");
        Ok(())
    }
    async fn delete(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    fn public_url(&self, key: &str) -> String {
        if self.absolute_urls {
            format!("https://storage.googleapis.com/bucket/{key}")
        } else {
            format!("/files/{key}")
        }
    }
    fn key_from_url(&self, _: &str) -> Option<String> {
        None
    }
}

/// Runs `create` with content embedding one attachment, and returns the journal.
async fn publish_post_with_attachment(
    data: Option<Vec<u8>>,
    absolute_urls: bool,
    put_fails: bool,
    increment_fails: bool,
) -> Vec<&'static str> {
    // `absolute_urls` doubles as "an object store is configured", which is the
    // arrangement `startup.rs` produces. `database_storage_with_a_cdn_promotes_nothing`
    // deliberately breaks that pairing, because breaking it is what caused a bug.
    publish_post_with(data, absolute_urls, put_fails, increment_fails, absolute_urls).await
}

async fn publish_post_with(
    data: Option<Vec<u8>>,
    absolute_urls: bool,
    put_fails: bool,
    increment_fails: bool,
    has_staging: bool,
) -> Vec<&'static str> {
    let thread_id = ids::thread_a();
    let cat_id = ids::category_a();
    let actor = member_with_post_perm();
    let thread = make_thread(thread_id, cat_id, ids::user_b());
    let category = make_category(cat_id);
    let post = make_post(ids::post_a(), thread_id, actor.id);

    let journal: Journal = Arc::new(Mutex::new(Vec::new()));

    let mut b = PostUseCaseBuilder::new();
    b.threads.expect_find_by_id().with(eq(thread_id)).return_once(move |_| Ok(Some(thread)));
    b.categories.expect_find_by_id().with(eq(cat_id)).return_once(move |_| Ok(Some(category)));
    b.posts.expect_create().return_once(move |_| Ok(post));
    b.threads.expect_update_reply_stats().return_once(|_, _, _| Ok(()));
    b.users.expect_increment_post_count().return_once(|_, _| Ok(()));
    b.events.expect_publish().return_once(|_| ());

    let stored_files = Arc::new(SpyStoredFiles {
        journal: journal.clone(),
        data,
        increment_fails,
    });
    let storage = Arc::new(SpyStorage {
        journal: journal.clone(),
        absolute_urls,
        put_fails,
    });
    let uc = b.build().with_stored_files(
        stored_files,
        storage.clone(),
        // Nothing in `create` uploads, so the same spy stands in for staging;
        // what matters is only whether it is present at all.
        has_staging.then_some(storage as Arc<dyn StorageService>),
    );

    let mut command = cmd(thread_id);
    command.content_md = format!("look: ![shot](/{ATTACHMENT_KEY})");
    uc.create(&actor, command).await.expect("post creation must succeed");

    let out = journal.lock().unwrap().clone();
    out
}

#[tokio::test]
async fn publishing_moves_the_attachment_out_before_dropping_the_only_copy() {
    // THE invariant. Reversing these two calls is not a style question: until
    // `put` lands, the database row holds the sole copy of the image, so
    // clearing first turns a transient object-store failure into permanent loss.
    let journal = publish_post_with_attachment(Some(vec![1, 2, 3]), true, false, false).await;

    assert_eq!(
        journal,
        vec!["increment_ref", "read_data", "put", "clear_data"],
        "bytes must reach the object store before the row lets go of them"
    );
}

#[tokio::test]
async fn a_failed_upload_leaves_the_bytes_in_the_database() {
    // Degrades to "this one image is served by the app instead of the bucket",
    // which is slower and entirely correct. Losing it would not be.
    let journal = publish_post_with_attachment(Some(vec![1, 2, 3]), true, true, false).await;

    assert!(
        !journal.contains(&"clear_data"),
        "clearing after a failed upload would destroy the only copy: {journal:?}"
    );
}

#[tokio::test]
async fn database_storage_promotes_nothing() {
    // No object store: staging and serving are the same place, so there is
    // nowhere to move the bytes to.
    let journal = publish_post_with_attachment(Some(vec![1, 2, 3]), false, false, false).await;

    assert_eq!(
        journal,
        vec!["increment_ref"],
        "no read, no put, no clear under database storage: {journal:?}"
    );
}

/// Why "is `public_url` absolute?" is an unsafe way to ask "is there an object
/// store?".
///
/// Database storage behind `CDN_BASE_URL` mints an absolute URL while the bytes
/// are still in the row, so promoting on that basis writes them back into the
/// row it just read and clears it — **destroying the only copy**.
///
/// `staging_storage` is the only valid signal.
#[tokio::test]
async fn database_storage_with_a_cdn_promotes_nothing() {
    let journal = publish_post_with(
        Some(vec![1, 2, 3]),
        /* absolute_urls */ true,
        /* put_fails */ false,
        /* increment_fails */ false,
        /* has_staging */ false,
    )
    .await;

    assert_eq!(
        journal,
        vec!["increment_ref"],
        "an absolute public_url is NOT evidence of an object store: {journal:?}"
    );
}

#[tokio::test]
async fn promotion_is_idempotent() {
    // An edit that re-adds the same image runs this path again. A row with no
    // bytes has already been promoted, and must not be re-uploaded.
    let journal = publish_post_with_attachment(None, true, false, false).await;

    assert_eq!(
        journal,
        vec!["increment_ref", "read_data"],
        "an already-promoted row stops at the read: {journal:?}"
    );
}

#[tokio::test]
async fn a_file_that_failed_to_publish_stays_staged_and_private() {
    // If `increment_ref` failed the attachment is not published, so promoting it
    // would push a still-staged file into a world-readable bucket — exactly the
    // exposure this whole arrangement exists to prevent.
    let journal = publish_post_with_attachment(Some(vec![1, 2, 3]), true, false, true).await;

    assert!(
        journal.is_empty(),
        "a failed increment must not promote: {journal:?}"
    );
}

// ─── list_by_author: category visibility ──────────────────────────────────────

/// `/api/users/:username/posts` is public and unauthenticated. Every sibling
/// read path (`ThreadUseCase::list_by_author`, the feed, search) intersects the
/// result with the categories the viewer may see; this one used to take no
/// viewer at all, so a guest could read the full body of every post made in a
/// `staff_only` category — the exact disclosure NF-SC-13 forbids.
#[tokio::test]
async fn list_by_author_excludes_categories_the_viewer_cannot_see() {
    let public = make_category(ids::category_a());
    let mut staff = make_category(ids::category_b());
    staff.slug = "staff".to_string();
    staff.view_policy = ferum_domain::models::category::ViewPolicy::StaffOnly;

    let mut b = PostUseCaseBuilder::new();
    b.categories
        .expect_list_all()
        .returning(move || Ok(vec![public.clone(), staff.clone()]));
    // The repository must be asked for the public category only — never for the
    // staff-only one.
    b.posts
        .expect_list_by_author()
        .withf(|_, category_ids, _, _| category_ids == [ids::category_a()])
        .times(1)
        .returning(|_, _, _, _| Ok((vec![], 0)));
    b.users.expect_find_by_id().returning(|_| Ok(None));

    let uc = b.build();
    uc.list_by_author(None, ids::user_a(), 1, 20).await.unwrap();
}

/// A guest who can see nothing must get nothing, not everything. The empty
/// allow-list is the fail-closed case and has to stay one.
#[tokio::test]
async fn list_by_author_returns_nothing_when_no_category_is_visible() {
    let mut staff = make_category(ids::category_b());
    staff.view_policy = ferum_domain::models::category::ViewPolicy::StaffOnly;

    let mut b = PostUseCaseBuilder::new();
    b.categories
        .expect_list_all()
        .returning(move || Ok(vec![staff.clone()]));
    b.posts
        .expect_list_by_author()
        .withf(|_, category_ids: &[Uuid], _, _| category_ids.is_empty())
        .returning(|_, _, _, _| Ok((vec![], 0)));
    b.users.expect_find_by_id().returning(|_| Ok(None));

    let uc = b.build();
    let (posts, total) = uc.list_by_author(None, ids::user_a(), 1, 20).await.unwrap();
    assert!(posts.is_empty());
    assert_eq!(total, 0);
}
