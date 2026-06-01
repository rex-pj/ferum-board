#![allow(dead_code)]

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::shared::AppError;

// ─── PasswordHasher ───────────────────────────────────────────────────────────

#[async_trait]
pub trait PasswordHasher: Send + Sync {
    async fn hash(&self, password: &str) -> Result<String, AppError>;
    async fn verify(&self, password: &str, hash: &str) -> Result<bool, AppError>;
}

// ─── TokenService ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait TokenService: Send + Sync {
    fn mint_access_token(&self, claims: &AccessTokenClaims) -> Result<String, AppError>;
    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, AppError>;
    fn mint_refresh_token(&self, user_id: Uuid) -> Result<String, AppError>;
    fn verify_refresh_token(&self, token: &str) -> Result<Uuid, AppError>;
    fn mint_email_token(&self, user_id: Uuid, purpose: &str) -> Result<String, AppError>;
    fn verify_email_token(&self, token: &str, expected_purpose: &str) -> Result<Uuid, AppError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: Uuid,
    pub username: String,
    pub role: String,
    pub trust_level: String,
    pub is_global_mod: bool,
    pub is_banned: bool,
    /// Unix timestamp of ban expiry; None means permanent ban.
    pub banned_until: Option<i64>,
    pub exp: i64,
}

// ─── CacheService ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait CacheService: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<(), AppError>;
    /// Atomically set the key only if it does not already exist (SET NX EX).
    /// Returns `true` if the key was newly set, `false` if it already existed.
    async fn set_nx(&self, key: &str, value: &str, ttl: Duration) -> Result<bool, AppError>;
    async fn del(&self, key: &str) -> Result<(), AppError>;
    /// Delete all keys whose name starts with `prefix`.
    async fn del_prefix(&self, prefix: &str) -> Result<(), AppError>;
    async fn incr_with_ttl(&self, key: &str, ttl: Duration) -> Result<u64, AppError>;
    async fn exists(&self, key: &str) -> bool;
}

// ─── RateLimiter ──────────────────────────────────────────────────────────────

#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check(
        &self,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<RateLimitResult, AppError>;
}

#[derive(Debug)]
pub enum RateLimitResult {
    Allowed { remaining: u32 },
    Denied { retry_after: Duration },
}

// ─── JobQueue ─────────────────────────────────────────────────────────────────

#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: ForumJob) -> Result<(), AppError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForumJob {
    SendEmailVerification {
        user_id: Uuid,
        email: String,
        token: String,
    },
    SendPasswordResetEmail {
        email: String,
        token: String,
    },
    SendNotificationEmail {
        user_id: Uuid,
        subject: String,
        body: String,
    },
    /// Decrement ref_count for a CAS key; delete from storage + DB if it hits 0.
    GcStorageKey {
        key: String,
    },
    SendWebhook {
        webhook_id: Uuid,
        url: String,
        secret: Option<String>,
        event_type: String,
        payload: serde_json::Value,
    },
}

// ─── StorageService ───────────────────────────────────────────────────────────

#[async_trait]
pub trait StorageService: Send + Sync {
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<(), AppError>;
    async fn delete(&self, key: &str) -> Result<(), AppError>;
    fn public_url(&self, key: &str) -> String;
}

// ─── SearchService ────────────────────────────────────────────────────────────

#[async_trait]
pub trait SearchService: Send + Sync {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError>;
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub q: String,
    pub category_id: Option<Uuid>,
    pub page: u64,
    pub per_page: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub thread_id: Uuid,
    pub thread_slug: String,
    pub title: String,
    pub excerpt: Option<String>,
}

// ─── NotificationBus ──────────────────────────────────────────────────────────

#[async_trait]
pub trait NotificationBus: Send + Sync {
    async fn publish(&self, user_id: Uuid, payload: serde_json::Value) -> Result<(), AppError>;
}

// ─── EmailService ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait EmailService: Send + Sync {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), AppError>;
}

// ─── BulkSeedService ──────────────────────────────────────────────────────────

#[async_trait]
pub trait BulkSeedService: Send + Sync {
    /// Seed example + bulk test data.  `admin_id` is the already-created admin
    /// user so example content can be authored by the real account.
    async fn seed_bulk(&self, admin_id: Uuid) -> Result<(), AppError>;
}
