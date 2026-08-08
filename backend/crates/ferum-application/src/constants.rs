pub const BCRYPT_COST: u32 = 12;
/// Fallback when the JWT_EXPIRY_SECONDS env var is not set.
pub const DEFAULT_JWT_EXPIRY_SECS: u64 = 3600; // 1 hour
/// Fallback when the REFRESH_TOKEN_EXPIRY_DAYS env var is not set.
pub const DEFAULT_REFRESH_TOKEN_EXPIRY_DAYS: u64 = 7;
pub const PASSWORD_RESET_TOKEN_TTL_SECS: u64 = 3600; // 1 hour

// ── Runtime-configurable defaults (fallback when site_config has no value) ──
// Admins can override these via admin/settings; these values are used when
// the site_config table has not been populated for the corresponding key.

pub const DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS: i32 = 5;
pub const DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES: u64 = 15;

pub const DEFAULT_POST_EDIT_WINDOW_HOURS: i64 = 24;

pub const DEFAULT_MAX_POSTS_PER_PAGE: u64 = 20;
pub const DEFAULT_MAX_THREADS_PER_PAGE: u64 = 30;

pub const DEFAULT_AUTH_RATE_LIMIT_PER_MIN: u32 = 10;
pub const DEFAULT_PUBLIC_WRITE_RATE_LIMIT_PER_MIN: u32 = 30;

/// Ceiling on `/files/` blob reads per client per minute.
///
/// An order of magnitude above the write limits because it paces a different
/// thing: a single thread page can pull dozens of avatars and thumbnails, so
/// this must clear ordinary browsing comfortably and only catch a client
/// enumerating the CAS namespace. Requests answered from cache with a 304 still
/// count, which is deliberate — the point is to bound request volume.
pub const DEFAULT_FILE_READ_RATE_LIMIT_PER_MIN: u32 = 300;

pub const DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY: u64 = 5;

// ── Fixed constants — not admin-configurable ─────────────────────────────────

pub const DEFAULT_THEME_SLUG: &str = "default";

/// Site name used before an admin sets one, e.g. in transactional email copy.
pub const DEFAULT_SITE_NAME: &str = "Ferum Board";

/// How deep `?page=` may go on any paginated endpoint.
///
/// Every list in this codebase paginates with `LIMIT/OFFSET`, so the database
/// must walk and discard every row before the offset. Without a ceiling,
/// `?page=999999` is an unauthenticated request that costs seconds of database
/// CPU — and because list queries run their COUNT and their data fetch
/// concurrently, one such request occupies two pool connections while it does.
///
/// 500 pages is past anything a human reaches by clicking; deep crawlers are
/// what actually generate these. Readers who need to go further have search.
/// Raising this trades directly against how cheaply the server can be stalled,
/// so it is deliberately not admin-configurable.
pub const MAX_PAGE: u64 = 500;

/// Categories are a two-level hierarchy: a top-level category and its children.
/// The check itself is structural (a parent may not already have a parent); this
/// names the resulting depth so the error message can state it without a literal.
pub const MAX_CATEGORY_DEPTH: usize = 2;

pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024; // 5 MB
pub const MAX_THUMBNAIL_BYTES: usize = 10 * 1024 * 1024; // 10 MB
pub const MAX_COVER_BYTES: usize = 8 * 1024 * 1024; // 8 MB
pub const MAX_PLUGIN_MEDIA_BYTES: usize = 8 * 1024 * 1024; // 8 MB
pub const MAX_FAVICON_BYTES: usize = 512 * 1024; // 512 KB
pub const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024; // 2 MB
pub const MAX_POST_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024; // 8 MB

/// Ceiling on an uploaded plugin package (`.fpkg`).
///
/// Three places enforce this — the two upload handlers, so an oversize body is
/// rejected before it is buffered, and `package_extractor`, which is the last
/// gate and cannot assume it was called through a handler. They were three
/// separate literals; the router's global `RequestBodyLimitLayer` is sized from
/// this ceiling too, so a change in one place and not the others silently makes
/// the backstop the real limit.
pub const MAX_PLUGIN_PACKAGE_BYTES: usize = 50 * 1024 * 1024; // 50 MB

// ── Per-account rolling upload quota ─────────────────────────────────────────
// The generic write rate limiter keys on IP, not account, so it does not bound
// what one account can store (nor stop one abuser rotating IPs). These cap the
// storage a single account can consume in a 24h window across every CAS
// namespace, making "upload forever, share the /files/ URL" unprofitable.
pub const UPLOAD_QUOTA_WINDOW_HOURS: i64 = 24;
pub const MAX_UPLOADS_PER_WINDOW: u64 = 50;
pub const MAX_UPLOAD_BYTES_PER_WINDOW: i64 = 100 * 1024 * 1024; // 100 MB

pub const MAX_POST_CONTENT_BYTES: usize = 100 * 1024; // 100 KB
pub const MAX_BAN_REASON_LEN: usize = 1_000;

/// Longest ban `moderation.ban_temp` can express.
///
/// This is what makes `moderation.ban_temp` and `admin.ban_permanent` two
/// different powers rather than two names for one. There was no ceiling, so a
/// moderator could pass `until = 9999-12-31` and end an account for good —
/// including an admin's — without holding `admin.ban_permanent` at all.
///
/// A year is well past any cooling-off purpose while still being a ban that
/// visibly expires. Anything longer is a permanent ban and should be asked for
/// as one, by someone who holds that permission.
pub const MAX_TEMP_BAN_DAYS: i64 = 365;

pub const MIN_THREAD_TITLE_LEN: usize = 5;
pub const MAX_THREAD_TITLE_LEN: usize = 255;
pub const MAX_TAGS_PER_THREAD: usize = 5;

// ── Notification retention ───────────────────────────────────────────────────
//
// Notifications had no retention at all, so the table grew for the life of an
// account — and `/notifications` runs a COUNT over every row a user has ever
// received, making the page's cost a function of account age rather than of
// what is on screen.
//
// Two windows because the two states carry different value. A read notification
// has already done its job: the user saw it, and the thread it points at is
// still reachable by other means. An unread one is a pending item nobody has
// seen, so it is kept far longer and removed only once it is old enough that
// nobody is realistically coming back for it.
pub const NOTIFICATION_READ_RETENTION_DAYS: u32 = 30;
pub const NOTIFICATION_UNREAD_RETENTION_DAYS: u32 = 180;

/// Auto-disable a webhook after this many consecutive failures.
pub const WEBHOOK_MAX_FAILURES: i32 = 5;

/// Most webhooks that will be dispatched for one event.
///
/// The event bus enqueues one background job per subscribed webhook, so this
/// row count multiplies the work a single post creates. Fifty is far beyond any
/// legitimate deployment; it exists so a misconfigured install cannot turn one
/// reply into hundreds of concurrent jobs competing for the connection pool.
pub const MAX_WEBHOOKS_PER_EVENT: u64 = 50;

/// Reviews a product needs before its rating panel shows the star-distribution
/// histogram and the per-dimension comparison bars.
///
/// Below this, both charts mislead rather than inform: a distribution over one
/// or two reviews is four empty rows, and sub-scores that are all equal render
/// as five identical bars implying a comparison that does not exist. Under the
/// threshold the panel falls back to the headline score plus plain sub-score
/// chips (see `product_rating_panel` in themes/default/templates/macros.html).
pub const MIN_REVIEWS_FOR_RATING_BREAKDOWN: i64 = 5;
