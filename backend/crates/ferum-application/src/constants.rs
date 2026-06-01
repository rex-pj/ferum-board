#![allow(dead_code)]

pub const BCRYPT_COST: u32 = 12;
pub const JWT_EXPIRY_SECS: u64 = 3600; // 1 hour
pub const REFRESH_TOKEN_TTL_SECS: u64 = 604_800; // 7 days
pub const PASSWORD_RESET_TOKEN_TTL_SECS: u64 = 3600; // 1 hour

pub const ACCOUNT_LOCKOUT_ATTEMPTS: i32 = 5;
pub const ACCOUNT_LOCKOUT_DURATION_MINUTES: u64 = 15;

pub const POST_EDIT_WINDOW_HOURS: i64 = 24;

pub const MAX_POSTS_PER_PAGE: u64 = 20;
pub const MAX_THREADS_PER_PAGE: u64 = 30;

pub const AUTH_RATE_LIMIT_PER_MIN: u32 = 10;
pub const PUBLIC_WRITE_RATE_LIMIT_PER_MIN: u32 = 30;

pub const APP_URL_DEFAULT: &str = "http://localhost:8080";

pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024; // 5 MB
pub const MAX_THUMBNAIL_BYTES: usize = 10 * 1024 * 1024; // 10 MB
pub const MAX_FAVICON_BYTES: usize = 512 * 1024; // 512 KB
pub const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024; // 2 MB

pub const MAX_POST_CONTENT_BYTES: usize = 100 * 1024; // 100 KB
pub const MAX_BAN_REASON_LEN: usize = 1_000;

/// Auto-disable a webhook after this many consecutive failures.
pub const WEBHOOK_MAX_FAILURES: i32 = 5;

/// Recent threads shown per category on the forum index page.
pub const FORUM_INDEX_THREADS_PER_CATEGORY: u64 = 5;
