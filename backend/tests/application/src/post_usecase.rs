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
// A staged attachment's bytes live in the database, never in the object store:
// while `ref_count == 0` it is authorized per viewer, and that is unenforceable
// once the object is world-readable. They move outward only when a post
// publishes them, via `promote_attachment`.
//
// Driven through `create` rather than by calling the method directly, so these
// also pin that promotion happens on the real publish path and only after
// `increment_ref` succeeds. `promote_attachment` stays private, which is what it
// should be — it is not something a handler may call.

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

/// The case that made "is `public_url` absolute?" an unsafe way to ask "is there
/// an object store?".
///
/// `DatabaseStorageService` behind `CDN_BASE_URL` mints
/// `https://cdn.example.com/files/{key}` — absolute, while the bytes are still
/// in the row. Promoting on that basis writes them back into the row they were
/// just read from and then clears it, **destroying the only copy**, and leaves
/// `/files/` redirecting to a CDN that fetches `/files/` straight back.
///
/// Presence of staging is now the only signal, so an absolute URL alone must not
/// trigger anything.
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
