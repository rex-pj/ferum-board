
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
    async fn verify<'a>(&self, password: &'a str, hash: &'a str) -> Result<bool, AppError>;
}

// ─── TokenService ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait TokenService: Send + Sync {
    fn mint_access_token(&self, claims: &AccessTokenClaims) -> Result<String, AppError>;
    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, AppError>;
    fn mint_refresh_token(&self, user_id: Uuid) -> Result<String, AppError>;
    fn verify_refresh_token(&self, token: &str) -> Result<Uuid, AppError>;
    fn mint_email_token(&self, user_id: Uuid, purpose: &str) -> Result<String, AppError>;
    fn verify_email_token<'a>(&self, token: &'a str, expected_purpose: &'a str) -> Result<Uuid, AppError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub trust_level: String,
    pub is_banned: bool,
    /// Unix timestamp of ban expiry; None means permanent ban.
    pub banned_until: Option<i64>,
    pub exp: i64,
}

// ─── CacheService ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait CacheService: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<(), AppError>;
    /// Atomically set the key only if it does not already exist (SET NX EX).
    /// Returns `true` if the key was newly set, `false` if it already existed.
    async fn set_nx<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<bool, AppError>;
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
    /// All category IDs to filter by (parent + resolved children). Empty = no filter.
    pub category_ids: Vec<Uuid>,
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

// ─── PluginRuntime ────────────────────────────────────────────────────────────

/// Context passed to every before-hook invocation.
/// Contains only what plugins are allowed to see — never raw DB connections or secrets.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookContext {
    pub hook_name: String,
    pub actor_id: Option<Uuid>,
    pub actor_trust_level: String,
    pub payload: serde_json::Value,
}

#[derive(Debug)]
pub enum HookDecision {
    Allow,
    Deny { reason: String, error_code: String },
}

/// Entry returned by active_ui_slots() for SSR frontend hydration.
#[derive(Debug, Clone)]
pub struct UiSlotEntry {
    pub slot_name: String,
    pub plugin_slug: String,
    pub asset_url: String,
    pub custom_element_tag: String,
    pub props: Vec<String>,
    pub load_order: i32,
}

/// ISP: Use cases (auth, post, thread) and EventBus only need hook dispatch.
#[async_trait]
pub trait PluginHookRuntime: Send + Sync {
    /// Dispatch a synchronous before-hook. Returns Allow unless a plugin explicitly denies.
    /// Plugin failures (timeout, crash) are logged and treated as Allow — never fail the request.
    async fn dispatch_before_hook(
        &self,
        hook: &str,
        ctx: &HookContext,
    ) -> Result<HookDecision, AppError>;

    /// Fire-and-forget after-event dispatch.
    /// Runs in a background task; errors are logged and never propagated to the caller.
    async fn dispatch_after_event(&self, event_type: &str, payload: serde_json::Value);
}

/// ISP: Template handlers need UI slot data — independent of hook execution.
#[async_trait]
pub trait PluginUiRuntime: Send + Sync {
    /// Returns all active UI slot registrations, sorted by load_order ASC.
    /// Used by the frontend SSR layer to hydrate plugin Web Components.
    async fn active_ui_slots(&self) -> Vec<UiSlotEntry>;
}

/// ISP: PluginUseCase needs lifecycle control — independent of hook or UI concerns.
#[async_trait]
pub trait PluginLifecycle: Send + Sync {
    /// Reload a plugin's in-memory dispatch table entry after activation or deactivation.
    /// Called by PluginUseCase after updating plugin status in DB.
    async fn reload_plugin(&self, plugin_id: Uuid);
}

/// No-op implementation used during startup or when plugin system is disabled.
pub struct NullPluginRuntime;

#[async_trait]
impl PluginHookRuntime for NullPluginRuntime {
    async fn dispatch_before_hook(
        &self,
        _hook: &str,
        _ctx: &HookContext,
    ) -> Result<HookDecision, AppError> {
        Ok(HookDecision::Allow)
    }

    async fn dispatch_after_event(&self, _event_type: &str, _payload: serde_json::Value) {}
}

#[async_trait]
impl PluginUiRuntime for NullPluginRuntime {
    async fn active_ui_slots(&self) -> Vec<UiSlotEntry> {
        vec![]
    }
}

#[async_trait]
impl PluginLifecycle for NullPluginRuntime {
    async fn reload_plugin(&self, _plugin_id: Uuid) {}
}

// ─── PermissionResolver ───────────────────────────────────────────────────────

use std::collections::{HashMap, HashSet};
use ferum_domain::models::role::UserRoleAssignment;

/// DIP: abstracts RolePermissionCache behind an application-layer port.
/// Web layer depends on this trait, not on the concrete infrastructure type.
#[async_trait]
pub trait PermissionResolver: Send + Sync {
    async fn resolve_global(&self, assignments: &[UserRoleAssignment]) -> HashSet<String>;
    async fn resolve_category(
        &self,
        assignments: &[UserRoleAssignment],
    ) -> HashMap<Uuid, HashSet<String>>;
    async fn permissions_for_role(&self, role_id: Uuid) -> Vec<String>;
    /// Reload the in-memory permission mapping from DB. Call after admin changes role permissions.
    async fn reload(&self) -> Result<(), AppError>;
}

// ─── NotificationSubscriber ───────────────────────────────────────────────────

/// DIP: abstracts SseBroadcaster behind an application-layer port.
/// The SSE handler depends on this trait, not on the concrete broadcaster type.
pub trait NotificationSubscriber: Send + Sync {
    /// Register a new SSE connection for `user_id`. Returns the receive end of
    /// the channel; the caller streams it to the HTTP response.
    fn subscribe(&self, user_id: Uuid) -> tokio::sync::mpsc::UnboundedReceiver<String>;
    fn active_connection_count(&self) -> usize;
}

