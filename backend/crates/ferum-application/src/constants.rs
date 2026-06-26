
pub const BCRYPT_COST: u32 = 12;
pub const JWT_EXPIRY_SECS: u64 = 3600; // 1 hour
pub const REFRESH_TOKEN_TTL_SECS: u64 = 604_800; // 7 days
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

pub const APP_URL_DEFAULT: &str = "http://localhost:8080";

pub const DEFAULT_THEME_SLUG: &str = "default";

pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024; // 5 MB
pub const MAX_THUMBNAIL_BYTES: usize = 10 * 1024 * 1024; // 10 MB
pub const MAX_COVER_BYTES: usize = 8 * 1024 * 1024; // 8 MB
pub const MAX_FAVICON_BYTES: usize = 512 * 1024; // 512 KB
pub const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024; // 2 MB

pub const MAX_POST_CONTENT_BYTES: usize = 100 * 1024; // 100 KB
pub const MAX_BAN_REASON_LEN: usize = 1_000;

pub const MIN_THREAD_TITLE_LEN: usize = 5;
pub const MAX_THREAD_TITLE_LEN: usize = 255;
pub const MAX_TAGS_PER_THREAD: usize = 5;

/// Auto-disable a webhook after this many consecutive failures.
pub const WEBHOOK_MAX_FAILURES: i32 = 5;
