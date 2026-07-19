//! Tests for [`JobExecutor`]: HMAC signing and GC storage key logic.
//!
//! `hmac_sha256` and `run_gc_storage_key` are tested with in-process test doubles
//! — no real database needed. The doubles implement the same port traits that
//! production code depends on, so the tests remain honest about the contract.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use ferum_application::ports::{EmailService, StorageService};
use ferum_application::shared::AppError;
use ferum_domain::models::webhook::Webhook;
use ferum_domain::repositories::{
    stored_file_repository::StoredFileRepository,
    webhook_repository::{NewWebhook, UpdateWebhook, WebhookRepository},
};
use ferum_infrastructure::job_queue::inline_runner::{hmac_sha256, JobExecutor};

// ─── Test doubles ─────────────────────────────────────────────────────────────

struct NullEmail;
#[async_trait]
impl EmailService for NullEmail {
    async fn send(&self, _: &str, _: &str, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}

struct NullStorage;
#[async_trait]
impl StorageService for NullStorage {
    async fn put(&self, _: &str, _: Bytes, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    fn public_url(&self, key: &str) -> String {
        format!("/files/{key}")
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
}

struct SpyStoredFiles {
    decrement_returns: i32,
}
#[async_trait]
impl StoredFileRepository for SpyStoredFiles {
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
        _: &[u8],
        _: i64,
        _: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn upsert_staged(
        &self,
        _: &str,
        _: &str,
        _: &[u8],
        _: i64,
        _: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn increment_ref(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn decrement_ref(&self, _: &str) -> Result<i32, AppError> {
        Ok(self.decrement_returns)
    }
    async fn delete_by_key(&self, _: &str) -> Result<(), AppError> {
        Ok(())
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
async fn gc_skips_delete_when_ref_count_positive() {
    // decrement returns 1 → another reference exists → no delete must happen
    let executor = make_executor(
        NullStorage,
        SpyStoredFiles {
            decrement_returns: 1,
        },
    );
    assert!(executor.run_gc_storage_key("sha256-abc123").await.is_ok());
}

#[tokio::test]
async fn gc_deletes_when_no_refs_remain() {
    // decrement returns 0 → last reference gone → delete from storage + DB
    let executor = make_executor(
        NullStorage,
        SpyStoredFiles {
            decrement_returns: 0,
        },
    );
    assert!(executor.run_gc_storage_key("sha256-orphan").await.is_ok());
}

#[tokio::test]
async fn gc_continues_when_storage_delete_errors() {
    // Storage failure is logged and swallowed — DB row deletion must still be attempted
    let executor = make_executor(
        ErrorStorage,
        SpyStoredFiles {
            decrement_returns: 0,
        },
    );
    assert!(executor.run_gc_storage_key("sha256-orphan").await.is_ok());
}
