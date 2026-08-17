pub const BCRYPT_COST: u32 = 12;
/// Fallback when JWT_EXPIRY_SECONDS is unset.
pub const DEFAULT_JWT_EXPIRY_SECS: u64 = 3600; // 1 hour
/// Fallback when REFRESH_TOKEN_EXPIRY_DAYS is unset.
pub const DEFAULT_REFRESH_TOKEN_EXPIRY_DAYS: u64 = 7;
pub const PASSWORD_RESET_TOKEN_TTL_SECS: u64 = 3600; // 1 hour

/// Shares the reset lifetime's value, not its meaning — a verification link is
/// followed once, and `resend_verification_email` covers anyone slower.
pub const EMAIL_VERIFICATION_TOKEN_TTL_SECS: u64 = 3600; // 1 hour

/// Deliberately nothing like the two above: this must still work whenever the
/// user next opens an old notification email. An expired unsubscribe link is
/// indistinguishable from a broken one, which is what turns "mute" into "spam".
pub const UNSUBSCRIBE_TOKEN_TTL_SECS: u64 = 365 * 24 * 3600; // 1 year

// ── Runtime-configurable defaults ────────────────────────────────────────────
// Used only when site_config holds no value for the key; admins override these
// from admin/settings.

pub const DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS: i32 = 5;
pub const DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES: u64 = 15;

pub const DEFAULT_POST_EDIT_WINDOW_HOURS: i64 = 24;

pub const DEFAULT_MAX_POSTS_PER_PAGE: u64 = 20;
pub const DEFAULT_MAX_THREADS_PER_PAGE: u64 = 30;

pub const DEFAULT_AUTH_RATE_LIMIT_PER_MIN: u32 = 10;
pub const DEFAULT_PUBLIC_WRITE_RATE_LIMIT_PER_MIN: u32 = 30;

/// An order of magnitude above the write limits: one thread page pulls dozens of
/// avatars, so this must clear ordinary browsing and only catch CAS enumeration.
/// 304s still count — the point is to bound request volume.
pub const DEFAULT_FILE_READ_RATE_LIMIT_PER_MIN: u32 = 300;

pub const DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY: u64 = 5;

// ── Upload image pipeline ────────────────────────────────────────────────────

/// Long-edge ceiling for images that are downscaled but never cropped — post
/// attachments, product and plugin media. Chosen so a full-width screenshot
/// stays legible; below ~1600 small UI text in a screenshot turns to mush.
pub const DEFAULT_IMAGE_MAX_LONG_EDGE: u32 = 2048;

/// JPEG quality for every lossy re-encode. 82 sits just under the point where
/// artefacts become visible on photographs while cutting a phone photo by
/// roughly 90%. Admin-tunable because the right answer depends on the forum's
/// subject matter — a photography board wants more than a support board.
pub const DEFAULT_IMAGE_JPEG_QUALITY: u32 = 82;

/// Hard pixel ceiling for any decoded upload, ~6300×6300.
///
/// **Not admin-configurable, and that is the point.** The `MAX_*_BYTES` limits
/// below bound the *file*, which says nothing about decoded size: a valid 400 KB
/// PNG can declare 50000×50000 and ask for ~10 GB. Raising this from the admin
/// panel would be handing out a denial-of-service switch.
pub const MAX_UPLOAD_MEGAPIXELS: u32 = 40;

/// Allocation ceiling handed to the decoder, as a second line behind
/// [`MAX_UPLOAD_MEGAPIXELS`]. The pixel check reads the header, which a crafted
/// file can lie about; this one binds the decoder itself.
pub const IMAGE_MAX_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

/// Output frames for the paths whose layout fixes the aspect ratio.
/// Avatars render at 32–96px but are stored at 2× the largest use so a retina
/// display has pixels to work with.
pub const AVATAR_FRAME: (u32, u32) = (512, 512);
pub const COVER_FRAME: (u32, u32) = (1600, 400);
pub const THUMBNAIL_FRAME: (u32, u32) = (1280, 720);

/// Long-edge ceiling for the site logo.
///
/// **1024, not 512, and the end-to-end run is why.** A 600×600 PNG logo hit the
/// old cap, got resized to 512 — and came out *larger*: 11.4 KB in, 12.7 KB out.
/// Flat colour is what PNG is best at, and Lanczos replaces a hard edge with a
/// band of anti-aliased pixels, each a new colour for the palette to carry. The
/// resize was real, so the no-growth guard correctly declined to help.
///
/// At 1024 an ordinary logo passes through untouched and the guard *does* apply,
/// which is the outcome worth having. The cap still exists to stop a 6000px
/// upload being served as the header image on every page.
pub const LOGO_MAX_LONG_EDGE: u32 = 1024;
pub const THEME_PREVIEW_MAX_LONG_EDGE: u32 = 1200;

/// IANA name defining the day boundary for daily aggregations. Never a fixed
/// offset — it cannot express DST. Changing it re-buckets future rows only, so a
/// chart spanning the change mixes two conventions.
pub const DEFAULT_REPORTING_TIMEZONE: &str = "UTC";

// ── site_config keys needed below the web layer ──────────────────────────────
// Most key strings live in handlers/admin/api/config.rs beside their allowlists.
// These are here because ferum-infrastructure needs them and cannot import
// ferum-web; that module re-exports them so call sites are unchanged.

pub const SMTP_HOST_KEY: &str = "smtp_host";
pub const SMTP_PORT_KEY: &str = "smtp_port";
pub const SMTP_USER_KEY: &str = "smtp_user";
pub const SMTP_PASS_KEY: &str = "smtp_pass";

/// `smtp` or `resend`. Selects a provider; it does not authenticate one —
/// Resend's key stays in the environment, out of every database backup.
pub const MAIL_PROVIDER_KEY: &str = "mail_provider";

/// In site_config, not env-only, because SPF and DKIM align against this domain
/// rather than the relay — being unable to change it is how mail starts landing
/// in spam with nothing in the UI to fix.
pub const FROM_EMAIL_KEY: &str = "from_email";

// ── Fixed constants — not admin-configurable ─────────────────────────────────

pub const DEFAULT_THEME_SLUG: &str = "default";

/// Used before an admin sets one, e.g. in transactional email.
pub const DEFAULT_SITE_NAME: &str = "Ferum Board";

/// `LIMIT/OFFSET` walks and discards every row before the offset, so an
/// unauthenticated `?page=999999` costs seconds of CPU on two pool connections.
/// Raising this trades against how cheaply the server can be stalled.
pub const MAX_PAGE: u64 = 500;

/// Names the depth the structural check produces (a parent may not have a
/// parent), so the error message needs no literal.
pub const MAX_CATEGORY_DEPTH: usize = 2;

pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024; // 5 MB
pub const MAX_THUMBNAIL_BYTES: usize = 10 * 1024 * 1024; // 10 MB
pub const MAX_COVER_BYTES: usize = 8 * 1024 * 1024; // 8 MB
pub const MAX_PLUGIN_MEDIA_BYTES: usize = 8 * 1024 * 1024; // 8 MB
pub const MAX_FAVICON_BYTES: usize = 512 * 1024; // 512 KB
pub const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024; // 2 MB
pub const MAX_POST_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024; // 8 MB

/// Enforced in three places — both upload handlers and `package_extractor` —
/// and the router sizes its global body limit from it. Change one and the
/// backstop silently becomes the real limit.
pub const MAX_PLUGIN_PACKAGE_BYTES: usize = 50 * 1024 * 1024; // 50 MB

// ── Per-account rolling upload quota ─────────────────────────────────────────
// The write rate limiter keys on IP, so it bounds neither what one account can
// store nor an abuser rotating IPs. These do, across every CAS namespace.
pub const UPLOAD_QUOTA_WINDOW_HOURS: i64 = 24;
pub const MAX_UPLOADS_PER_WINDOW: u64 = 50;
pub const MAX_UPLOAD_BYTES_PER_WINDOW: i64 = 100 * 1024 * 1024; // 100 MB

pub const MAX_POST_CONTENT_BYTES: usize = 100 * 1024; // 100 KB
pub const MAX_BAN_REASON_LEN: usize = 1_000;

/// Written twice — the `validate` attribute and the textarea's `maxlength` — and
/// they had drifted, so the form was the real limit. The template now renders
/// this constant instead of repeating a literal.
pub const MAX_REPORT_REASON_LEN: usize = 500;

/// What makes `moderation.ban_temp` and `admin.ban_permanent` different powers.
/// With no ceiling a moderator passes `until = 9999-12-31` and bans permanently
/// without holding the permission.
pub const MAX_TEMP_BAN_DAYS: i64 = 365;

/// `per_page` ceiling for lists with no admin-configurable page size.
///
/// **A handler's `paginate` ceiling must equal its use case's.** If they differ,
/// rows are served at the lower one while `total_pages` is computed from the
/// higher, so the last pages advertise rows nothing can reach — silently.
pub const MAX_LIST_PAGE_SIZE: u64 = 50;

pub const MIN_THREAD_TITLE_LEN: usize = 5;
pub const MAX_THREAD_TITLE_LEN: usize = 255;
pub const MAX_TAGS_PER_THREAD: usize = 5;

// ── Notification retention ───────────────────────────────────────────────────
// `/notifications` COUNTs every row an account ever received, so without this
// page cost becomes a function of account age. Two windows because a read
// notification has done its job and an unread one is still pending.
pub const NOTIFICATION_READ_RETENTION_DAYS: u32 = 30;
pub const NOTIFICATION_UNREAD_RETENTION_DAYS: u32 = 180;

/// Auto-disable a webhook after this many consecutive failures.
pub const WEBHOOK_MAX_FAILURES: i32 = 5;

/// The event bus enqueues one job per subscribed webhook, so this bounds how far
/// a single post fans out. Far beyond any legitimate deployment.
pub const MAX_WEBHOOKS_PER_EVENT: u64 = 50;

/// Below this the histogram and comparison bars mislead — a distribution over
/// two reviews is four empty rows — so the panel falls back to the headline
/// score plus plain chips.
pub const MIN_REVIEWS_FOR_RATING_BREAKDOWN: i64 = 5;
