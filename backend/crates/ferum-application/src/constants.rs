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

pub const DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY: u64 = 5;

// ── Fixed constants — not admin-configurable ─────────────────────────────────

pub const APP_URL_DEFAULT: &str = "http://localhost:5173";

pub const DEFAULT_THEME_SLUG: &str = "default";

/// Site name used before an admin sets one, e.g. in transactional email copy.
pub const DEFAULT_SITE_NAME: &str = "Ferum Board";

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

pub const MIN_THREAD_TITLE_LEN: usize = 5;
pub const MAX_THREAD_TITLE_LEN: usize = 255;
pub const MAX_TAGS_PER_THREAD: usize = 5;

/// Auto-disable a webhook after this many consecutive failures.
pub const WEBHOOK_MAX_FAILURES: i32 = 5;

/// Reviews a product needs before its rating panel shows the star-distribution
/// histogram and the per-dimension comparison bars.
///
/// Below this, both charts mislead rather than inform: a distribution over one
/// or two reviews is four empty rows, and sub-scores that are all equal render
/// as five identical bars implying a comparison that does not exist. Under the
/// threshold the panel falls back to the headline score plus plain sub-score
/// chips (see `product_rating_panel` in themes/default/templates/macros.html).
pub const MIN_REVIEWS_FOR_RATING_BREAKDOWN: i64 = 5;
