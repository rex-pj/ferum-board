//! Tests for [`JobExecutor`]: HMAC signing and GC storage key logic.
//!
//! `hmac_sha256` and `run_gc_storage_key` are tested with in-process test doubles
//! — no real database needed. The doubles implement the same port traits that
//! production code depends on, so the tests remain honest about the contract.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use ferum_application::ports::{
    EmailService, ForumJob, NotificationEmailKind, OutgoingEmail, StorageService,
};
use ferum_application::shared::AppError;
use ferum_domain::models::webhook::Webhook;
use ferum_domain::Locale;
use ferum_domain::repositories::{
    stored_file_repository::StoredFileRepository,
    webhook_repository::{NewWebhook, UpdateWebhook, WebhookRepository},
};
use ferum_domain::models::email_template::EmailTemplate;
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_infrastructure::email::DbEmailTemplateRenderer;
use ferum_infrastructure::job_queue::inline_runner::{hmac_sha256, InlineJobRunner, JobExecutor};
use ferum_test_support::mocks::email_service::{RecordingEmailService, SentEmail};
use ferum_test_support::mocks::token_service::MockTokenService;

// ─── Test doubles ─────────────────────────────────────────────────────────────

struct NullEmail;
#[async_trait]
impl EmailService for NullEmail {
    async fn send(&self, _: OutgoingEmail<'_>) -> Result<(), AppError> {
        Ok(())
    }
}

/// Records whether the blob was actually removed, so a test can distinguish
/// "GC ran and deleted" from "GC ran and correctly did nothing".
#[derive(Clone, Default)]
struct SpyStorage {
    deleted: Arc<AtomicBool>,
}
#[async_trait]
impl StorageService for SpyStorage {
    async fn put(&self, _: &str, _: Bytes, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete(&self, _: &str) -> Result<(), AppError> {
        self.deleted.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn public_url(&self, key: &str) -> String {
        format!("/files/{key}")
    }
    fn key_from_url(&self, url: &str) -> Option<String> {
        url.strip_prefix("/files/").map(str::to_string)
    }
}

struct ErrorStorage;
#[async_trait]
impl StorageService for ErrorStorage {
    async fn put(&self, _: &str, _: Bytes, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete(&self, _: &str) -> Result<(), AppError> {
        Err(AppError::internal("storage unavailable"))
    }
    fn public_url(&self, key: &str) -> String {
        format!("/files/{key}")
    }
    fn key_from_url(&self, url: &str) -> Option<String> {
        url.strip_prefix("/files/").map(str::to_string)
    }
}

/// `delete_if_unreferenced_returns` stands in for what the conditional DELETE
/// finds in the database: `true` = the row was still at ref_count 0 and went,
/// `false` = something re-referenced it (or it was already gone).
///
/// `decrement_calls` exists to pin down the contract, not the mechanism: the GC
/// job must never decrement, because the caller already did. See the doc on
/// `ForumJob::GcStorageKey`.
#[derive(Clone)]
struct SpyStoredFiles {
    delete_if_unreferenced_returns: bool,
    decrement_calls: Arc<AtomicUsize>,
}
impl SpyStoredFiles {
    fn new(delete_if_unreferenced_returns: bool) -> Self {
        Self {
            delete_if_unreferenced_returns,
            decrement_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}
#[async_trait]
impl StoredFileRepository for SpyStoredFiles {
    async fn attachment_keys_before(&self, _: chrono::DateTime<chrono::Utc>) -> Result<Vec<String>, AppError> { Ok(vec![]) }
    async fn referencing_pointers(&self) -> Result<Vec<String>, AppError> { Ok(vec![]) }
    async fn ref_counts_for(&self, _: &[String]) -> Result<Vec<(String, i32)>, AppError> { Ok(vec![]) }
    async fn usage_since(
        &self,
        _: uuid::Uuid,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<ferum_domain::repositories::stored_file_repository::UploadUsage, AppError> {
        Ok(
            ferum_domain::repositories::stored_file_repository::UploadUsage {
                file_count: 0,
                total_bytes: 0,
            },
        )
    }
    async fn upsert_and_ref(
        &self,
        _: &str,
        _: &str,
        _: i64,
        _: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn upsert_staged(
        &self,
        _: &str,
        _: &str,
        _: i64,
        _: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn increment_ref(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn read_data(&self, _: &str) -> Result<Option<(Vec<u8>, String)>, AppError> {
        Ok(None)
    }
    async fn clear_data(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn decrement_ref(&self, _: &str) -> Result<i32, AppError> {
        self.decrement_calls.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }
    async fn delete_if_unreferenced(&self, _: &str) -> Result<bool, AppError> {
        Ok(self.delete_if_unreferenced_returns)
    }
    async fn list_keys_with_prefix(&self, _: &str) -> Result<Vec<String>, AppError> {
        Ok(vec![])
    }
}

struct NullWebhooks;
#[async_trait]
impl WebhookRepository for NullWebhooks {
    async fn list(&self) -> Result<Vec<Webhook>, AppError> {
        Ok(vec![])
    }
    async fn find_by_id(&self, _: uuid::Uuid) -> Result<Option<Webhook>, AppError> {
        Ok(None)
    }
    async fn find_subscribed(&self, _: &str) -> Result<Vec<Webhook>, AppError> {
        Ok(vec![])
    }
    async fn create(&self, _: NewWebhook) -> Result<Webhook, AppError> {
        unimplemented!()
    }
    async fn update(&self, _: uuid::Uuid, _: UpdateWebhook) -> Result<Webhook, AppError> {
        unimplemented!()
    }
    async fn delete(&self, _: uuid::Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete_by_plugin(&self, _: uuid::Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn record_success(&self, _: uuid::Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn record_failure(&self, _: uuid::Uuid) -> Result<(), AppError> {
        Ok(())
    }
}

fn make_executor(
    storage: impl StorageService + 'static,
    stored_files: impl StoredFileRepository + 'static,
) -> JobExecutor {
    JobExecutor::new(
        Arc::new(NullEmail),
        "http://localhost:5173".to_string(),
        Arc::new(storage),
        Arc::new(stored_files),
        Arc::new(NullWebhooks),
    )
}

// ─── hmac_sha256 ──────────────────────────────────────────────────────────────

#[test]
fn hmac_sha256_produces_64_char_hex() {
    let digest = hmac_sha256("mysecret", r#"{"event":"post.created"}"#);
    assert_eq!(digest.len(), 64, "SHA-256 hex digest is always 64 chars");
}

#[test]
fn hmac_sha256_is_deterministic() {
    let a = hmac_sha256("key", "payload");
    let b = hmac_sha256("key", "payload");
    assert_eq!(a, b);
}

#[test]
fn hmac_sha256_differs_on_different_secret() {
    let a = hmac_sha256("secret1", "payload");
    let b = hmac_sha256("secret2", "payload");
    assert_ne!(a, b);
}

#[test]
fn hmac_sha256_only_hex_chars() {
    let digest = hmac_sha256("test", "{}");
    assert!(!digest.is_empty());
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
}

// ─── run_gc_storage_key ───────────────────────────────────────────────────────

#[tokio::test]
async fn gc_deletes_blob_when_row_was_still_unreferenced() {
    let storage = SpyStorage::default();
    let files = SpyStoredFiles::new(true);
    let executor = make_executor(storage.clone(), files.clone());

    executor.run_gc_storage_key("sha256-orphan").await.unwrap();

    assert!(
        storage.deleted.load(Ordering::SeqCst),
        "row went, so the blob behind it must go too"
    );
}

/// The regression this whole change exists for.
///
/// A user removes an avatar (ref_count → 0, GC enqueued) and re-uploads the
/// identical file before the job runs. CAS deduplicates on content, so the
/// upload revives the very same row at ref_count 1. The conditional DELETE
/// therefore matches nothing, and GC must leave the blob alone — the old code
/// decremented that fresh reference back to 0 and deleted the file the user had
/// just uploaded.
#[tokio::test]
async fn gc_leaves_blob_alone_when_key_was_re_referenced() {
    let storage = SpyStorage::default();
    let files = SpyStoredFiles::new(false);
    let executor = make_executor(storage.clone(), files.clone());

    executor.run_gc_storage_key("sha256-revived").await.unwrap();

    assert!(
        !storage.deleted.load(Ordering::SeqCst),
        "a re-referenced key must keep its blob"
    );
}

/// The caller owns the decrement; the job re-testing by decrementing again is
/// exactly what destroyed the revived reference above.
#[tokio::test]
async fn gc_never_decrements_ref_count() {
    let files = SpyStoredFiles::new(true);
    let executor = make_executor(SpyStorage::default(), files.clone());

    executor.run_gc_storage_key("sha256-orphan").await.unwrap();

    assert_eq!(
        files.decrement_calls.load(Ordering::SeqCst),
        0,
        "GC must not decrement — callers decrement before enqueueing"
    );
}

#[tokio::test]
async fn gc_continues_when_storage_delete_errors() {
    // A backend blob that refuses to go is logged and swallowed: the row is
    // already deleted, so failing here would only retry a delete that cannot
    // succeed. The leak is an orphaned blob, which is recoverable; the
    // alternative — a live row pointing at a blob we deleted — is not.
    let executor = make_executor(ErrorStorage, SpyStoredFiles::new(true));
    assert!(executor.run_gc_storage_key("sha256-orphan").await.is_ok());
}

// ─── Job budget classification ────────────────────────────────────────────────

/// Every `ForumJob` variant, with the budget it should draw from.
///
/// Hand-written, so this list alone does not force completeness — what does is
/// `is_network_bound` being an exhaustive `match`, which fails to compile on a
/// new variant. These assert the classification it then makes.
fn all_job_variants() -> Vec<(&'static str, ForumJob, bool)> {
    let uuid = uuid::Uuid::nil();
    vec![
        (
            "SendEmailVerification",
            ForumJob::SendEmailVerification {
                user_id: uuid,
                email: "a@b.c".into(),
                token: "t".into(),
                locale: Locale::default(),
            },
            false,
        ),
        (
            "SendPasswordResetEmail",
            ForumJob::SendPasswordResetEmail {
                email: "a@b.c".into(),
                token: "t".into(),
                locale: Locale::default(),
            },
            false,
        ),
        (
            "SendNotificationEmail",
            ForumJob::SendNotificationEmail {
                user_id: uuid,
                email: "member@example.com".into(),
                kind: NotificationEmailKind::Reply,
                thread_slug: "a-thread".into(),
                thread_title: "A thread".into(),
                actor_username: "someone".into(),
                locale: Locale::default(),
            },
            false,
        ),
        ("GcStorageKey", ForumJob::GcStorageKey { key: "k".into() }, false),
        (
            "SendWebhook",
            ForumJob::SendWebhook {
                webhook_id: uuid,
                url: "https://example.invalid/hook".into(),
                secret: None,
                event_type: "post.created".into(),
                payload: serde_json::json!({}),
            },
            true,
        ),
    ]
}

#[test]
fn only_webhook_delivery_draws_from_the_network_budget() {
    for (name, job, expected) in all_job_variants() {
        assert_eq!(
            InlineJobRunner::is_network_bound(&job),
            expected,
            "{name} is classified into the wrong job budget"
        );
    }
}

/// Guards the reason the split exists: webhooks fan out (one per subscriber for
/// a single post) and can each sit in the network for the full 10s HTTP timeout.
/// Sharing one budget meant a `SendEmailVerification` queued behind a burst of
/// them waited minutes, so a stalled third-party integration delayed signups.
#[test]
fn email_is_never_classified_with_webhooks() {
    let emails: Vec<_> = all_job_variants()
        .into_iter()
        .filter(|(name, _, _)| name.starts_with("Send") && name.contains("mail"))
        .collect();
    assert!(!emails.is_empty(), "the email variants should still exist");
    for (name, job, _) in emails {
        assert!(
            !InlineJobRunner::is_network_bound(&job),
            "{name} must not share the webhook budget"
        );
    }
}

// ─── HTML escaping in notification bodies ─────────────────────────────────────

/// A store with nothing in it, so the renderer falls back to the compiled-in
/// catalogue. That keeps this suite free of a database while still exercising
/// the real copy, the real substitution and the real escaping — a stubbed
/// renderer would prove nothing about any of them.
struct NoStoredTemplates;
#[async_trait]
impl EmailTemplateRepository for NoStoredTemplates {
    async fn list(&self) -> Result<Vec<EmailTemplate>, AppError> {
        Ok(vec![])
    }
    async fn find(&self, _: &str, _: &str) -> Result<Option<EmailTemplate>, AppError> {
        Ok(None)
    }
    async fn resolve(&self, _: &str, _: &[String]) -> Result<Option<EmailTemplate>, AppError> {
        Ok(None)
    }
    async fn upsert(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn insert_if_absent(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn delete(&self, _: &str, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}

/// Runs one `SendNotificationEmail` and returns what the provider was handed.
async fn send_notification(thread_title: &str, actor: &str) -> SentEmail {
    let mail = Arc::new(RecordingEmailService::new());
    let renderer = Arc::new(DbEmailTemplateRenderer::new(Arc::new(NoStoredTemplates)));

    let mut tokens = MockTokenService::new();
    tokens
        .expect_mint_email_token()
        .returning(|_, _, _| Ok("unsub-token".to_string()));

    let executor = JobExecutor::new(
        mail.clone(),
        "http://localhost:5173".to_string(),
        Arc::new(SpyStorage::default()),
        Arc::new(SpyStoredFiles::new(false)),
        Arc::new(NullWebhooks),
    )
    .with_email_templates(renderer, "Ferum Board".to_string())
    .with_tokens(Arc::new(tokens));

    executor
        .run(ForumJob::SendNotificationEmail {
            user_id: uuid::Uuid::nil(),
            email: "member@example.com".into(),
            kind: NotificationEmailKind::Reply,
            thread_slug: "a-thread".into(),
            thread_title: thread_title.into(),
            actor_username: actor.into(),
            locale: Locale::default(),
        })
        .await
        .expect("the notification job should send");

    mail.sent().pop().expect("exactly one message should be sent")
}

/// The bug this guards: Fluent performs no escaping and `set_use_isolating(false)`
/// is set, so a thread title reached the HTML body verbatim. Titles are checked
/// for length only (`validate_thread_title`) and never pass through ammonia, so
/// an author could put a working `<a href>` in a stranger's inbox — sent from the
/// forum's own verified sending domain, which is what makes it a phishing vector
/// rather than cosmetic breakage.
#[tokio::test]
async fn an_author_controlled_title_cannot_inject_markup_into_the_body() {
    let sent = send_notification(r#"<a href="https://evil.example">Click</a>"#, "someone").await;

    assert!(
        !sent.html_body.contains("<a href=\"https://evil.example\""),
        "the title's anchor must not survive as markup:\n{}",
        sent.html_body
    );
    assert!(
        sent.html_body.contains("&lt;a href=&quot;https://evil.example&quot;&gt;"),
        "the title should appear escaped instead:\n{}",
        sent.html_body
    );
}

#[tokio::test]
async fn an_author_controlled_username_cannot_inject_markup_into_the_body() {
    let sent = send_notification("A thread", "<script>alert(1)</script>").await;

    assert!(
        !sent.html_body.contains("<script>"),
        "the username's script tag must not survive as markup:\n{}",
        sent.html_body
    );
}

/// The other half of the fix, and the reason the two arg sets exist. A subject
/// line is plain text, so escaping it there would show a literal `&amp;` in the
/// recipient's inbox list — a regression that is invisible to the body tests.
#[tokio::test]
async fn the_subject_keeps_its_punctuation_unescaped() {
    let sent = send_notification("Bells & Whistles", "someone").await;

    assert!(
        sent.subject.contains("Bells & Whistles"),
        "the subject is plain text and must not be HTML-escaped: {}",
        sent.subject
    );
}

/// A text part that is really the HTML would satisfy any check that only asked
/// whether one was present, and would hand a plain-text reader a page of tags.
#[tokio::test]
async fn the_plain_text_part_is_actually_plain() {
    let sent = send_notification("A thread", "alice").await;

    assert!(!sent.text_body.is_empty(), "a text alternative must be sent");
    assert_ne!(sent.text_body, sent.html_body);
    assert!(
        !sent.text_body.contains('<') && !sent.text_body.contains('>'),
        "no markup may survive into the text part: {}",
        sent.text_body
    );
    // The link is the entire point of a notification mail, so it has to be
    // reachable without an HTML renderer.
    assert!(
        sent.text_body.contains("http://localhost:5173/forum/t/a-thread"),
        "the text part must carry the link target: {}",
        sent.text_body
    );
}

/// Escaping is per part, so the two can disagree — and only the HTML one may
/// carry entities.
#[tokio::test]
async fn the_text_part_shows_a_title_unescaped() {
    let sent = send_notification("Bells & Whistles", "alice").await;

    assert!(
        sent.text_body.contains("Bells & Whistles"),
        "plain text needs no entities: {}",
        sent.text_body
    );
    assert!(
        sent.html_body.contains("Bells &amp; Whistles"),
        "the HTML part still escapes: {}",
        sent.html_body
    );
}
