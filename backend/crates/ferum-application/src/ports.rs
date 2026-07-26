
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use ferum_domain::models::product::ProductType;
use ferum_domain::Locale;
/// Re-exported so port consumers get the argument type alongside the trait.
/// It lives in the domain because `AppError` carries translation arguments and
/// the domain cannot depend on this crate.
pub use ferum_domain::TransArg;

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

    /// Lifetime of minted access tokens. Every consumer (claims `exp`, cookie
    /// Max-Age) must read this instead of hardcoding a duration, so the token
    /// and its cookie can never disagree.
    fn access_token_ttl_secs(&self) -> u64 {
        crate::constants::DEFAULT_JWT_EXPIRY_SECS
    }

    /// Lifetime of minted refresh tokens (token `exp`, cache-key TTL, cookie Max-Age).
    fn refresh_token_ttl_secs(&self) -> u64 {
        crate::constants::DEFAULT_REFRESH_TOKEN_EXPIRY_DAYS * 86_400
    }
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
    /// Unix timestamp the token was issued at.
    ///
    /// Access tokens are stateless, so there is otherwise no way to stop one:
    /// logging out or changing a password left every already-issued token valid
    /// until `exp`. Comparing this against a per-user "session epoch" (bumped on
    /// logout and password change) gives revocation without adding a session
    /// table. See `session_epoch_key` and the check in the auth middleware.
    ///
    /// Defaulted so tokens minted before this field existed still deserialize;
    /// such a token reads as `iat = 0` and is therefore treated as predating any
    /// epoch that gets set — it is revoked the first time a user actually
    /// invalidates their sessions, which is the correct, fail-safe direction.
    #[serde(default)]
    pub iat: i64,
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
        /// Language to write the email in.
        ///
        /// **Resolved at enqueue time and carried here on purpose.** The worker
        /// runs detached from the request, so by the time it executes there is no
        /// request locale to consult — and even if there were, it would be the
        /// *actor's*, not the recipient's. A Vietnamese member replying to an
        /// English member's thread must produce an English notification.
        locale: Locale,
    },
    SendPasswordResetEmail {
        email: String,
        token: String,
        locale: Locale,
    },
    SendNotificationEmail {
        user_id: Uuid,
        subject: String,
        body: String,
        locale: Locale,
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

/// What a query is searching over. One call answers for exactly one kind: the
/// relevance scores of a thread title and a product name are not on a common
/// scale, and merging them into one ranked list produces an order that means
/// nothing. Callers that want both run both and present them as distinct
/// sections — see `SearchUseCase::search_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    Thread,
    Product,
}

/// Ordering for product results.
///
/// Deliberately not `ProductSort` from the domain: browsing a catalogue has no
/// notion of relevance, and search has no business defaulting to "newest". The
/// two enums overlap but their defaults are opposites, which is exactly the
/// kind of thing that goes wrong silently when one type serves both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProductSearchSort {
    #[default]
    Relevance,
    TopRated,
    MostReviewed,
    Newest,
}

impl ProductSearchSort {
    pub fn as_str(self) -> &'static str {
        match self {
            ProductSearchSort::Relevance => "relevance",
            ProductSearchSort::TopRated => "top_rated",
            ProductSearchSort::MostReviewed => "most_reviewed",
            ProductSearchSort::Newest => "newest",
        }
    }

    /// Unknown values fall back to relevance rather than erroring — a stale or
    /// hand-edited `?psort=` should still return results.
    pub fn parse(s: &str) -> Self {
        match s {
            "top_rated" => ProductSearchSort::TopRated,
            "most_reviewed" => ProductSearchSort::MostReviewed,
            "newest" => ProductSearchSort::Newest,
            _ => ProductSearchSort::Relevance,
        }
    }
}

/// Ordering for thread (discussion) results.
///
/// Separate from the domain's `ThreadSort` (feed browsing) for the same reason
/// [`ProductSearchSort`] is separate from `ProductSort`: search defaults to
/// relevance, a feed defaults to recency, and the feed's `Latest`/`Unanswered`/
/// `Solved` modes have no meaning over a text-match result set. Kept minimal on
/// purpose — three orderings the searcher actually reaches for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThreadSearchSort {
    #[default]
    Relevance,
    Newest,
    MostReplies,
}

impl ThreadSearchSort {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadSearchSort::Relevance => "relevance",
            ThreadSearchSort::Newest => "newest",
            ThreadSearchSort::MostReplies => "most_replies",
        }
    }

    /// Unknown values fall back to relevance, mirroring [`ProductSearchSort::parse`].
    pub fn parse(s: &str) -> Self {
        match s {
            "newest" => ThreadSearchSort::Newest,
            "most_replies" => ThreadSearchSort::MostReplies,
            _ => ThreadSearchSort::Relevance,
        }
    }
}

/// Product-only narrowing, applied on top of the text match.
///
/// The forum category is deliberately *not* here — it narrows both kinds and
/// lives on [`SearchQuery::in_category_ids`]. Everything in this struct is
/// meaningless for a thread, and keeping the split explicit is what stops a
/// brand filter from silently shrinking the discussion list.
#[derive(Debug, Clone, Default)]
pub struct ProductFacets {
    pub product_type: Option<ProductType>,
    pub brand_id: Option<Uuid>,
    pub material_id: Option<Uuid>,
    pub sort: ProductSearchSort,
}

impl ProductFacets {
    /// Whether any narrowing facet is set. Sort is excluded — reordering is not
    /// filtering, and a page that shows "filters active" because someone picked
    /// an ordering is lying to the reader.
    pub fn is_narrowed(&self) -> bool {
        self.product_type.is_some() || self.brand_id.is_some() || self.material_id.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub q: String,
    pub kind: SearchKind,
    pub facets: ProductFacets,
    /// Ordering for thread hits. Ignored when `kind` is `Product` (products use
    /// `facets.sort`). Defaults to relevance, so a caller that does not set it
    /// gets the text-rank order that was the only option before.
    pub thread_sort: ThreadSearchSort,
    /// The exact set of categories a thread hit may come from — already
    /// intersected with what the viewer is allowed to see. **Empty yields no
    /// thread hits**, which is the safe default: a caller that forgets to
    /// resolve visibility gets nothing rather than leaking the titles of
    /// `staff_only` threads. Ignored when `kind` is `Product`.
    ///
    /// Named for what it is, because the field below has the *opposite* empty
    /// semantics and one of them has to fail closed.
    pub visible_category_ids: Vec<Uuid>,
    /// The category the reader chose, expanded to include its children.
    /// **Empty means no restriction** — unlike `visible_category_ids`, this is a
    /// user filter, not an access control, so its absence must widen rather
    /// than narrow.
    ///
    /// Applied to products only; the thread side gets the same narrowing by
    /// having `visible_category_ids` intersected with the choice before it
    /// arrives here. Products with no category are excluded when this is set —
    /// an unfiled product is genuinely not in any category, and including it
    /// everywhere would make the filter meaningless.
    pub in_category_ids: Vec<Uuid>,
    /// Who is asking. Draft products are unlisted, except to whoever submitted
    /// them — otherwise a contributor cannot find their own pending entry.
    /// Ignored when `kind` is `Thread`.
    pub viewer_id: Option<Uuid>,
    pub page: u64,
    pub per_page: u64,
    /// Return only `total`, with no hits. Tab badges need the size of the kind
    /// the user is *not* currently looking at ("Products (12)"), and fetching
    /// rows that will be discarded to learn a number is waste. `page` and
    /// `per_page` are ignored when this is set.
    pub count_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub total: u64,
}

/// A bare hit: enough to identify and link the match, no more. Display data
/// (author, rating, price, cover) is joined on afterwards by the use case,
/// which owns the repositories — a search backend should not be a second,
/// stale copy of the domain.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub kind: SearchKind,
    /// Thread id or product id, per `kind`.
    pub id: Uuid,
    pub slug: String,
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

// ─── WebhookDeliveryService ───────────────────────────────────────────────────

pub struct WebhookTestResult {
    pub success: bool,
    pub status_code: Option<u16>,
    /// Set on transport-level failure (DNS/connect/timeout) or on an SSRF-guard
    /// rejection — status_code is None in that case since no response arrived.
    pub error: Option<String>,
}

#[async_trait]
pub trait WebhookDeliveryService: Send + Sync {
    /// Sends a single synchronous test POST (event type "test") to `url`, using
    /// the same SSRF-safe DNS-pinning and HMAC signing as real event delivery,
    /// but without touching the webhook's failure_count/last_triggered_at —
    /// a deliberate test isn't a real delivery attempt for health-tracking purposes.
    async fn send_test(&self, url: &str, secret: Option<&str>) -> Result<WebhookTestResult, AppError>;
}

// ─── HostResolver ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait HostResolver: Send + Sync {
    /// Resolves `host` (a bare hostname or an IP literal) to its addresses.
    ///
    /// Exists so the application layer can reject a webhook URL whose *hostname*
    /// points at a private address — something an IP-literal check cannot see.
    /// This is admin-facing validation, not the security boundary: the boundary
    /// is `build_pinned_client`, which re-resolves and pins at dispatch time.
    /// DNS can change between the two, so this check can only ever be advisory.
    async fn resolve(&self, host: &str) -> Result<Vec<std::net::IpAddr>, AppError>;
}

// ─── BulkSeedService ──────────────────────────────────────────────────────────

#[async_trait]
pub trait BulkSeedService: Send + Sync {
    /// Seed example + bulk test data.  `admin_id` is the already-created admin
    /// user so example content can be authored by the real account.
    async fn seed_bulk(&self, admin_id: Uuid) -> Result<(), AppError>;
}

/// Used when the `bulk_seed` feature is compiled out — see
/// `ferum-infrastructure`'s Cargo.toml for why a production build would want
/// that. Refuses rather than silently succeeding: the setup wizard's checkbox
/// is a promise to create demo content, so a build that cannot keep it must say
/// so instead of finishing setup with an empty forum and no explanation.
pub struct NullBulkSeedService;

#[async_trait]
impl BulkSeedService for NullBulkSeedService {
    async fn seed_bulk(&self, _admin_id: Uuid) -> Result<(), AppError> {
        Err(AppError::invalid("seed_data_unavailable"))
    }
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

/// ISP: the RPC HTTP handler needs to invoke a plugin's registered RPC actions —
/// independent of hook dispatch, UI, or lifecycle concerns.
#[async_trait]
pub trait PluginRpcRuntime: Send + Sync {
    /// Invoke a named RPC action registered by a Script-tier plugin.
    /// Unlike `dispatch_before_hook` (fail-open), RPC failures propagate as real
    /// errors — the caller is an HTTP client waiting on a genuine response, not
    /// a use case observing a side effect. Implementations must reject any
    /// action not present in BOTH the plugin's manifest and its granted
    /// capabilities before ever reaching the plugin's code.
    async fn dispatch_rpc(
        &self,
        plugin_slug: &str,
        action: &str,
        ctx: &HookContext,
    ) -> Result<serde_json::Value, AppError>;
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

#[async_trait]
impl PluginRpcRuntime for NullPluginRuntime {
    async fn dispatch_rpc(
        &self,
        _plugin_slug: &str,
        _action: &str,
        _ctx: &HookContext,
    ) -> Result<serde_json::Value, AppError> {
        Err(AppError::NotFound)
    }
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


// ─── Translator ───────────────────────────────────────────────────────────────


/// Resolves a message key to text in a given locale.
///
/// DIP: abstracts the Fluent catalog behind an application-layer port, so use
/// cases and the web layer never name a `fluent-bundle` type. The argument type
/// is deliberately `TransArg` rather than Fluent's own `FluentArgs` — leaking
/// that type upward would make `ferum-application` depend on the very crate the
/// port exists to hide.
///
/// `translate` is **synchronous by contract**. It is called from inside Tera's
/// `register_function` closure, which is sync, so an async lookup here would be
/// unimplementable without blocking. Implementations must therefore keep the
/// catalog in memory and never perform I/O on this path — refreshing from disk
/// belongs in `reload`.
#[async_trait]
pub trait Translator: Send + Sync {
    /// Resolves `key` in `locale`, interpolating `args`.
    ///
    /// **Never fails.** A missing key walks the locale's fallback chain, then
    /// the default locale, and finally renders the key itself. A template or an
    /// error path must not be able to take down a page because a translation is
    /// absent — a visibly untranslated string is a far better failure mode than
    /// a 500, and it is self-diagnosing in a way an empty string is not.
    fn translate(&self, locale: &Locale, key: &str, args: &[(&str, TransArg)]) -> String;

    /// Whether `key` resolves in `locale` *without* falling back to another
    /// locale. Backs the admin coverage report, which is only meaningful if a
    /// key inherited from the default locale counts as missing.
    fn has_key(&self, locale: &Locale, key: &str) -> bool;

    /// Every locale with a loaded catalog, default first. This is the installed
    /// roster, not the admin-enabled one — enabling is site config layered on top.
    fn available_locales(&self) -> Vec<Locale>;

    /// Every key defined in the default locale. The canonical key set that
    /// coverage percentages are computed against.
    fn default_locale_keys(&self) -> Vec<String>;

    /// Re-reads catalogs from disk and atomically swaps them in. Called after a
    /// language pack upload or an admin string override, mirroring
    /// `PermissionResolver::reload` and `TeraEngine::reload_themes`.
    async fn reload(&self) -> Result<(), AppError>;
}
