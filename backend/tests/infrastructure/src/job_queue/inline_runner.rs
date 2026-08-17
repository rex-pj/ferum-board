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
    EmailService, ForumJob, NotificationEmailKind, StorageService,
};
use ferum_application::shared::AppError;
use ferum_domain::models::webhook::Webhook;
use ferum_domain::Locale;
use ferum_domain::repositories::{
    stored_file_repository::StoredFileRepository,
    webhook_repository::{NewWebhook, UpdateWebhook, WebhookRepository},
};
use ferum_infrastructure::job_queue::inline_runner::{hmac_sha256, InlineJobRunner, JobExecutor};

// ─── Test doubles ─────────────────────────────────────────────────────────────

struct NullEmail;
#[async_trait]
impl EmailService for NullEmail {
    async fn send(&self, _: &str, _: &str, _: &str) -> Result<(), AppError> {
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
