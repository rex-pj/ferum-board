
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
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

/// Hashes passwords and checks them.
///
/// Async because bcrypt is CPU-bound for hundreds of ms: implementations must
/// move it off the runtime (`spawn_blocking`) or it stalls a tokio worker.
#[async_trait]
pub trait PasswordHasher: Send + Sync {
    /// # Errors
    /// [`AppError::Internal`] if the hash cannot be produced.
    async fn hash(&self, password: &str) -> Result<String, AppError>;
    /// A wrong password is `Ok(false)`. `Err` means the check could not run —
    /// never treat it as a failed login, or a storage fault locks out accounts.
    ///
    /// # Errors
    /// [`AppError::Internal`] if the stored hash is unparseable.
    async fn verify<'a>(&self, password: &'a str, hash: &'a str) -> Result<bool, AppError>;
}

// ─── TokenService ─────────────────────────────────────────────────────────────

/// The `purpose` claim on an unsubscribe token.
///
/// A constant because the string is written in two places that must agree — the
/// job runner mints it, the unsubscribe handler verifies it — and
/// `verify_email_token` rejects a mismatch by returning "invalid or expired". A
/// typo would therefore present as every unsubscribe link in every email being
/// broken, with the error message pointing at expiry rather than at the typo.
pub const UNSUBSCRIBE_PURPOSE: &str = "unsubscribe";

/// Mints and verifies access, refresh, and single-purpose email tokens.
///
/// Verification errors are deliberately opaque: never report "expired" and
/// "signature invalid" differently, or an attacker learns which half was right.
#[async_trait]
pub trait TokenService: Send + Sync {
    /// # Errors
    /// [`AppError::Internal`] if signing fails.
    fn mint_access_token(&self, claims: &AccessTokenClaims) -> Result<String, AppError>;
    /// Checks signature and expiry only — **not revocation**. The
    /// `iat`-vs-session-epoch check in the auth middleware is what makes logout
    /// and password change take effect before `exp`.
    ///
    /// # Errors
    /// [`AppError::Unauthorized`] for any invalid token.
    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, AppError>;
    /// # Errors
    /// [`AppError::Internal`] if signing fails.
    fn mint_refresh_token(&self, user_id: Uuid) -> Result<String, AppError>;
    /// # Errors
    /// [`AppError::Unauthorized`] for any invalid token.
    fn verify_refresh_token(&self, token: &str) -> Result<Uuid, AppError>;
    /// Mints a single-purpose token for a link sent by email.
    ///
    /// **`ttl_secs` is a parameter rather than a constant inside the
    /// implementation, and that is load-bearing.** It used to be hardcoded to the
    /// password-reset lifetime for every purpose, which is right for a reset and
    /// wrong for an unsubscribe link: an email sits in an inbox for months, and an
    /// unsubscribe that has expired is indistinguishable from one that does not
    /// work — which is what gets a sending domain reported rather than merely
    /// muted. Making the caller name the lifetime forces the question to be
    /// answered per purpose instead of inherited by accident.
    fn mint_email_token(
        &self,
        user_id: Uuid,
        purpose: &str,
        ttl_secs: u64,
    ) -> Result<String, AppError>;
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

/// The JWT payload carried by the auth cookie.
///
/// A snapshot taken at sign-in, not live state — a ban applied now is invisible
/// in a token minted a minute ago, which is why use cases re-check it. Signed,
/// not encrypted: never add a field the token's bearer should not read.
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
    /// Issued-at. Compared against a per-user session epoch (bumped on logout
    /// and password change) to revoke stateless tokens without a session table.
    ///
    /// Defaulted, so a token predating this field reads `iat = 0` and is
    /// revoked by the first invalidation — the fail-safe direction.
    #[serde(default)]
    pub iat: i64,
}

// ─── CacheService ─────────────────────────────────────────────────────────────

/// Best-effort key/value store with TTLs (Redis, or an in-process map).
///
/// Any key may vanish at any time, so nothing may depend on one being present.
/// The in-process fallback is per-process: with two instances, entries are not
/// shared and an invalidation on one never reaches the other.
#[async_trait]
pub trait CacheService: Send + Sync {
    /// `None` also means "cache unreachable" — Redis errors degrade to a miss,
    /// so this cannot detect an outage. Callers needing that must not use it.
    async fn get(&self, key: &str) -> Option<String>;
    /// # Errors
    /// [`AppError::Internal`] if the backend rejected the write.
    async fn set<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<(), AppError>;
    /// Atomically set the key only if it does not already exist (SET NX EX).
    /// Returns `true` if newly set. The atomicity is the point — a
    /// `get`-then-`set` pair lets two concurrent requests both see the miss.
    ///
    /// # Errors
    /// [`AppError::Internal`] on backend failure; unlike `get` this does not
    /// degrade to a default.
    async fn set_nx<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<bool, AppError>;
    /// # Errors
    /// [`AppError::Internal`] if the backend rejected the delete.
    async fn del(&self, key: &str) -> Result<(), AppError>;
    /// Delete all keys starting with `prefix`. **Not atomic**: Redis walks the
    /// keyspace with SCAN, so a key written during the walk can survive.
    ///
    /// # Errors
    /// [`AppError::Internal`] if the scan or any delete failed.
    async fn del_prefix(&self, prefix: &str) -> Result<(), AppError>;
    /// TTL is set on creation and never extended — a fixed window, not sliding.
    ///
    /// # Errors
    /// [`AppError::Internal`] if the backend rejected the increment.
    async fn incr_with_ttl(&self, key: &str, ttl: Duration) -> Result<u64, AppError>;
    /// Same ambiguity as [`get`](CacheService::get): `false` may mean unreachable.
    async fn exists(&self, key: &str) -> bool;
}

// ─── RateLimiter ──────────────────────────────────────────────────────────────

/// Fixed-window request counter.
///
/// Fixed, not sliding: a burst straddling a window boundary can pass up to
/// twice `limit`. Accepted — this is spam control, not quota enforcement.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// This call *is* the increment, so calling it twice per request spends two.
    ///
    /// # Errors
    /// [`AppError::Internal`] when the backend is unreachable. The middleware
    /// **fails closed** (503), so a Redis outage makes rate-limited endpoints
    /// unavailable rather than unlimited.
    async fn check(
        &self,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<RateLimitResult, AppError>;
}

/// Outcome of a [`RateLimiter::check`].
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed { remaining: u32 },
    /// `retry_after` reaches the client as the `Retry-After` header.
    Denied { retry_after: Duration },
}

// ─── JobQueue ─────────────────────────────────────────────────────────────────

/// Hands work off so it does not block the response.
///
/// **Not durable.** `InlineJobRunner` is `tokio::spawn`, so a restart drops
/// everything pending and nothing is retried. Only for work whose loss is an
/// inconvenience — never for what the request's correctness depends on.
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// `Ok` means accepted, not done — later failure appears only in the log.
    ///
    /// # Errors
    /// [`AppError::Internal`] if the job could not be accepted; callers should
    /// log and continue rather than fail the request.
    async fn enqueue(&self, job: ForumJob) -> Result<(), AppError>;
}

/// Which in-app notification an email copy is for.
///
/// Deliberately narrower than `NotificationKind`: only the two kinds that are ever
/// emailed appear, so the runner's match is total and adding a third kind to the
/// notification system cannot silently start mailing people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationEmailKind {
    Reply,
    Mention,
}

impl NotificationEmailKind {
    /// Fluent key stem. The catalog defines `{stem}-subject` and `{stem}-body`.
    pub fn key_stem(&self) -> &'static str {
        match self {
            NotificationEmailKind::Reply => "email-notify-reply",
            NotificationEmailKind::Mention => "email-notify-mention",
        }
    }
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
    /// An email copy of an in-app notification.
    ///
    /// Carries the *facts*, not a rendered subject and body. The two are not
    /// interchangeable: rendering needs `app_url` and the translator, which live
    /// with the job runner, and the unsubscribe link needs a freshly minted token,
    /// which must not be built at enqueue time and then sit in a queue.
    ///
    /// Enqueued only after `EventBus` has checked the recipient's
    /// `EmailNotificationPrefs`, so reaching the runner already means "this person
    /// asked for this". The runner does not re-check.
    SendNotificationEmail {
        /// The recipient, resolved at enqueue time along with the decision to
        /// send — the runner must not have to look a user up to know where to
        /// deliver.
        user_id: Uuid,
        email: String,
        kind: NotificationEmailKind,
        thread_slug: String,
        thread_title: String,
        /// Who replied or did the mentioning.
        actor_username: String,
        locale: Locale,
    },
    /// Delete a CAS key's row and blob, but only if it is still unreferenced.
    ///
    /// The caller owns the decrement and enqueues this only after `ref_count`
    /// reached 0. The job must not decrement again — doing so would consume a
    /// reference taken between enqueue and execution, which CAS content
    /// deduplication makes an ordinary occurrence rather than a rare race.
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
    /// Where the bytes for `key` physically live, right now, under the backend
    /// currently configured.
    ///
    /// **This is a delivery address, not an identity.** It changes when the
    /// backend changes, when a CDN is put in front, when a bucket is renamed.
    /// Use it to *serve* a request — see [`file_url`] for what to persist.
    fn public_url(&self, key: &str) -> String;

    /// Inverse of [`StorageService::public_url`]. `None` when the URL is not
    /// ours — an author may paste any external image into a post.
    ///
    /// **Every implementation must ALSO accept `/files/{key}`**, whatever shape
    /// it emits: that is what gets persisted. Recognising only its own output
    /// silently stops matching, and since this drives ref counting, files get
    /// collected while posts still point at them.
    fn key_from_url(&self, url: &str) -> Option<String>;
}

/// The same-origin resolver path. Every backend must understand it, and
/// [`file_url`] is the only thing that should build it.
pub const FILES_PREFIX: &str = "/files/";

/// The stable **identity** of a stored file: `/files/{key}`.
///
/// PERSIST THIS, never [`StorageService::public_url`], which says only where
/// bytes live *today*. Anything written into `site_config`, `themes.preview_url`
/// or post HTML is never rewritten, so storing a delivery URL bakes in the
/// current bucket/CDN/backend and none of the three can change afterwards.
pub fn file_url(key: &str) -> String {
    format!("{FILES_PREFIX}{key}")
}

// ─── ImageProcessor ───────────────────────────────────────────────────────────

/// What an upload path wants done to an image.
///
/// Chosen per *feature*, never per file: whether cropping is legitimate is a
/// property of the layout the image lands in, not of the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePolicy {
    /// The layout dictates the frame — crop to exactly `width`×`height`.
    /// A card grid is uniform only if every image in it is.
    FixedFrame {
        width: u32,
        height: u32,
        quality: u8,
    },
    /// The image *is* the content: never cropped, downscaled only past
    /// `max_long_edge`. A post attachment is what its author meant to show, so
    /// choosing a different framing for them destroys the point of it.
    Preserve { max_long_edge: u32, quality: u8 },
    /// Re-encoded without discarding a pixel. Logos carry alpha, flat colour
    /// and thin text — the three things lossy compression visibly ruins.
    LosslessOnly { max_long_edge: u32 },
}

/// A crop chosen by the user, in source-image pixels.
///
/// **A hint, never a command.** Implementations clamp it to the decoded
/// dimensions and reject a zero-area result, and must add with `checked_add`:
/// an `x + w` that overflows otherwise wraps into a plausible-looking rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Bytes plus the content type that now describes them.
///
/// **Carry both to `cas_key`.** The key's extension comes from the content
/// type, so re-encoding to a new format while reusing the old one yields a
/// `.png` key holding JPEG bytes — a mismatch `key_from_url` still matches, so
/// nothing in this process detects it and only the browser refuses the image.
pub struct ProcessedImage {
    pub data: Bytes,
    pub content_type: String,
}

/// Hand-written so a failed assertion prints a size and a type rather than
/// several megabytes of escaped image data — `Bytes`' own `Debug` would dump
/// the whole buffer into the test output.
impl std::fmt::Debug for ProcessedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessedImage")
            .field("bytes", &self.data.len())
            .field("content_type", &self.content_type)
            .finish()
    }
}

/// Decodes, re-frames and re-encodes an uploaded image.
///
/// Async although the work is pure CPU: the implementation owns `spawn_blocking`
/// and the concurrency limit, so that resource policy exists once rather than at
/// every upload call site.
#[async_trait]
pub trait ImageProcessor: Send + Sync {
    /// # Errors
    /// `image_too_many_pixels` when the header declares more pixels than the
    /// ceiling — checked before allocating, since a byte-size cap says nothing
    /// about decoded size. `image_decode_failed` on malformed input or a decoder
    /// panic. `image_crop_invalid` on a rect that clamps to nothing.
    async fn process(
        &self,
        data: Bytes,
        content_type: &str,
        policy: ImagePolicy,
        crop: Option<CropRect>,
    ) -> Result<ProcessedImage, AppError>;
}

/// Hands every image back exactly as it arrived.
///
/// Selected when the `image_processing` feature is compiled out or an admin has
/// turned `image_processing_enabled` off. Degrading to the pre-pipeline
/// behaviour is the right failure mode — refusing uploads outright over a
/// storage optimisation would be worse than storing the bytes as they came.
pub struct PassthroughImageProcessor;

#[async_trait]
impl ImageProcessor for PassthroughImageProcessor {
    async fn process(
        &self,
        data: Bytes,
        content_type: &str,
        _policy: ImagePolicy,
        _crop: Option<CropRect>,
    ) -> Result<ProcessedImage, AppError> {
        Ok(ProcessedImage {
            data,
            content_type: content_type.to_string(),
        })
    }
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
/// `translate` is **synchronous by contract** — it runs inside Tera's sync
/// `register_function` closure, so implementations must hold the catalog in
/// memory and never do I/O here. Reloading from disk belongs in `reload`.
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

    /// The `js-` dictionary served in `<meta name="ferum-i18n">`.
    ///
    /// An `Arc` because it must be built **once per catalog load, not per
    /// render** — it is a pure function of (catalogs, locale), and both change
    /// only on `reload`. Building it per request meant cloning every key and
    /// running a Fluent format per `js-` key on every page.
    ///
    /// Keyed on the default locale's key set, so the shape is the same in every
    /// language and untranslated keys use the normal fallback chain.
    fn js_strings(&self, locale: &Locale) -> Arc<BTreeMap<String, String>>;

    /// Re-reads catalogs from disk and atomically swaps them in. Called after a
    /// language pack upload or an admin string override, mirroring
    /// `PermissionResolver::reload` and `TeraEngine::reload_themes`.
    async fn reload(&self) -> Result<(), AppError>;
}
