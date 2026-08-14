use serde::Serialize;

use crate::middleware::AuthUser;
use ferum_application::constants::MAX_PAGE;
use ferum_domain::repositories::thread_repository::{ThreadFeedFilter, ThreadSort};

/// A single rendered plugin Web Component element for a UI slot.
/// Pre-rendered as HTML so Tera can output it with `{{ slot.html | safe }}`.
#[derive(Serialize, Clone)]
pub struct PluginSlotCtx {
    /// Ready-to-render HTML, e.g. `<my-plugin-widget data-foo="bar"></my-plugin-widget>`
    pub html: String,
}

/// Server-enforced field limits, injected into every public page as `limits`.
///
/// A `maxlength` attribute and the validator behind it are the same rule
/// written twice, and they had already drifted: the report textarea stopped at
/// 500 while `CreateReportRequest` accepted 2000. Templates render
/// `{{ limits.report_reason }}` so there is one number, and a change to the
/// constant reaches the form without anyone remembering to follow it.
///
/// Only limits a template actually needs belong here — this is not a mirror of
/// `constants.rs`.
#[derive(Serialize, Clone)]
pub struct FieldLimitsCtx {
    pub report_reason: usize,
}

impl FieldLimitsCtx {
    pub fn new() -> Self {
        Self {
            report_reason: ferum_application::constants::MAX_REPORT_REASON_LEN,
        }
    }
}

impl Default for FieldLimitsCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal site info injected into every page.
#[derive(Serialize, Clone)]
pub struct SiteCtx {
    pub name: String,
    pub slogan: String,
    pub tagline: String,
    pub logo_url: Option<String>,
    pub favicon_url: Option<String>,
    pub primary_color: Option<String>,
    /// "R, G, B" string for Bootstrap's --bs-primary-rgb variable.
    pub primary_color_rgb: Option<String>,
    /// Absolute site origin, no trailing slash — lets templates build absolute
    /// URLs (required for og:image/twitter:image; relative URLs there are
    /// silently ignored by most social crawlers).
    pub url: String,
}

/// Current authenticated user context for templates.
#[derive(Serialize, Clone)]
pub struct CurrentUserCtx {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub is_moderator: bool,
    pub unread_count: u64,
    /// "light" | "dark" | "auto". Set from the DB-backed preference by
    /// `user_ctx()` so base.html can render the right `data-bs-theme` in the
    /// initial HTML — not left to client-side localStorage alone, which only
    /// carries over within the same browser/device.
    pub theme: String,
    pub font_size: String,
    pub layout: String,
    /// IANA zone this user chose for displaying timestamps, or `None` to follow
    /// whatever zone their device reports.
    ///
    /// Rendered into `<meta name="ferum-tz">` and read by `Ferum.tz()`. Carried
    /// server-side rather than left to localStorage — like `theme` above and for
    /// the same reason — so the choice follows the account to another device
    /// instead of living in one browser.
    pub timezone: Option<String>,
}

impl CurrentUserCtx {
    pub fn from_auth(u: &AuthUser, unread_count: u64) -> Self {
        let is_admin = u.has_perm("admin.users");
        // Mirrors handlers::moderation::require_moderator exactly, so a moderator
        // scoped to just one category (not global) still sees the Mod nav link.
        let is_moderator = u.has_perm_any_category("moderation.view_reports") || is_admin;
        Self {
            id: u.id.to_string(),
            username: u.username.clone(),
            display_name: u
                .display_name
                .clone()
                .unwrap_or_else(|| u.username.clone()),
            avatar_url: u.avatar_url.clone(),
            is_admin,
            is_moderator,
            unread_count,
            theme: "auto".to_string(),
            font_size: "medium".to_string(),
            layout: "comfortable".to_string(),
            timezone: None,
        }
    }
}

/// Builds the context from the JWT alone — no database read, so no
/// `unread_count` and, importantly, **no `timezone`**.
///
/// The admin and moderator panels use this. To give those pages the viewer's
/// stored zone, wrap the result with
/// [`crate::handlers::pages::with_viewer_timezone`] rather than reaching for
/// `user_ctx`, which would also pay for an unread-count query the panels do not
/// render.
impl From<&AuthUser> for CurrentUserCtx {
    fn from(u: &AuthUser) -> Self {
        Self::from_auth(u, 0)
    }
}

// ─── Thread feed controls ─────────────────────────────────────────────────────
//
// The listing has two independent axes — ORDER (sort tabs) and NARROWING (filter
// chips) — and every link has to carry the axis the reader did *not* just touch,
// or clicking a sort silently discards their filter. Building those URLs here
// rather than in Tera is deliberate on three counts: the rule has one definition
// instead of one per theme, it is unit-testable without rendering anything, and
// a theme author cannot get it subtly wrong while still producing a page that
// looks right.

/// Query-string tail carrying every non-default axis, each part keeping its
/// leading `&` so callers can append it after an existing `?page=N`.
///
/// Defaults are omitted so the default view stays at the bare URL instead of a
/// second address rendering identical content.
pub fn feed_query_tail(
    sort: ThreadSort,
    filter: ThreadFeedFilter,
    tag: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(tag) = tag {
        parts.push(format!("&tag={tag}"));
    }
    if sort != ThreadSort::default() {
        parts.push(format!("&sort={}", sort.as_str()));
    }
    if filter != ThreadFeedFilter::default() {
        parts.push(format!("&filter={}", filter.as_str()));
    }
    parts.join("")
}

/// A full listing URL. Also the 301 target when a request arrives spelled the
/// pre-split way.
pub fn feed_url(
    base: &str,
    sort: ThreadSort,
    filter: ThreadFeedFilter,
    tag: Option<&str>,
) -> String {
    match feed_query_tail(sort, filter, tag).strip_prefix('&') {
        Some(rest) => format!("{base}?{rest}"),
        None => base.to_string(),
    }
}

/// One control in the feed's tab strip or chip row.
#[derive(Serialize, Clone)]
pub struct FeedControlCtx {
    /// Wire value of this axis — `activity`, `unanswered`, …
    pub key: String,
    /// Where clicking goes. For a chip that is already `on`, this is the URL
    /// with the filter REMOVED — that is what makes it a toggle without a line
    /// of JavaScript.
    pub href: String,
    /// `ui-*` catalog key for the visible label.
    pub label_key: String,
    /// `ui-*` catalog key for the tooltip that says what the query really does.
    pub hint_key: String,
    /// FontAwesome class.
    pub icon: String,
    pub active: bool,
}

/// Both axes of the thread feed's controls, ready to render.
#[derive(Serialize, Clone)]
pub struct FeedControlsCtx {
    /// Reorder the list. Nothing disappears.
    pub sorts: Vec<FeedControlCtx>,
    /// Narrow the list. Rows disappear.
    pub filters: Vec<FeedControlCtx>,
}

impl FeedControlsCtx {
    pub fn build(base_url: &str, sort: ThreadSort, filter: ThreadFeedFilter) -> Self {
        let sorts = [
            (ThreadSort::Activity, "fa-clock-rotate-left", "ui-sort-activity"),
            (ThreadSort::Newest, "fa-calendar-plus", "ui-sort-newest"),
            (ThreadSort::MostReplies, "fa-comments", "ui-sort-most-discussed"),
        ]
        .into_iter()
        .map(|(candidate, icon, label_key)| FeedControlCtx {
            key: candidate.as_str().to_string(),
            // The active filter rides along, so changing the order never
            // silently drops the narrowing the reader asked for.
            href: feed_url(base_url, candidate, filter, None),
            label_key: label_key.to_string(),
            hint_key: format!("{label_key}-hint"),
            icon: icon.to_string(),
            active: candidate == sort,
        })
        .collect();

        let filters = [
            (ThreadFeedFilter::Unanswered, "fa-comment-slash", "ui-unanswered", "ui-filter-unanswered-hint"),
            (ThreadFeedFilter::Solved, "fa-circle-check", "ui-solved", "ui-filter-solved-hint"),
        ]
        .into_iter()
        .map(|(candidate, icon, label_key, hint_key)| {
            let on = candidate == filter;
            FeedControlCtx {
                key: candidate.as_str().to_string(),
                // Pressing an active chip clears it; the sort survives either way.
                href: feed_url(
                    base_url,
                    sort,
                    if on { ThreadFeedFilter::All } else { candidate },
                    None,
                ),
                label_key: label_key.to_string(),
                hint_key: hint_key.to_string(),
                icon: icon.to_string(),
                active: on,
            }
        })
        .collect();

        Self { sorts, filters }
    }
}

/// Pagination context for list pages.
#[derive(Serialize, Clone)]
pub struct PaginationCtx {
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
    pub total_pages: u64,
    pub has_prev: bool,
    pub has_next: bool,
    /// Extra query-string params (e.g. "&sort=newest&tag=rust") appended to page links
    /// so that pagination navigation preserves active filters.
    pub extra_params: String,
}

impl PaginationCtx {
    pub fn new(page: u64, per_page: u64, total: u64, extra_params: String) -> Self {
        // `per_page` is clamped before the arithmetic, not just in the divisor:
        // with per_page = 0 (`?per_page=0`) and total = 0, `total + per_page - 1`
        // underflows u64 — a panic in debug, u64::MAX in release. Handlers clamp
        // their query params too; this is the backstop for every call site.
        let per_page = per_page.max(1);
        // Capped at the same ceiling `utils::paginate` enforces on input.
        // Without this the navigation happily links to pages the server now
        // refuses: a forum with 50k threads at 20/page advertises 2,500 pages,
        // every one past 500 answering `page_out_of_range`. The reader would
        // meet a dead link the UI drew for them.
        //
        // `total` is deliberately NOT capped — "1,234 threads" stays true, and
        // it is what the page actually reports; only reachability is bounded.
        //
        // The macro also loops `range(1..=total_pages)` to render a five-link
        // window, so this incidentally stops that being a 2,500-iteration loop
        // on every render.
        let total_pages = total.div_ceil(per_page).min(MAX_PAGE);
        Self {
            page,
            per_page,
            total,
            total_pages,
            has_prev: page > 1,
            has_next: page < total_pages,
            extra_params,
        }
    }

    /// Convenience constructor with no extra params (thread detail, search, etc.).
    pub fn simple(page: u64, per_page: u64, total: u64) -> Self {
        Self::new(page, per_page, total, String::new())
    }
}

/// Thread summary for list pages.
#[derive(Serialize, Clone)]
pub struct ThreadCtx {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub author_username: String,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    pub category_slug: String,
    pub category_name: String,
    /// True when this thread lives in the canonical Reviews category — lets list
    /// cards flag reviews apart from ordinary discussions.
    pub is_review: bool,
    /// The review's overall star score (1–5), when this is a review with a rating.
    pub review_overall: Option<i16>,
    /// Reviewed product's cover image key — a card thumbnail fallback for reviews
    /// that have no thumbnail of their own.
    pub review_product_image: Option<String>,
    /// Reviewed product's name and slug, so a review card can say what it is a
    /// review *of* and link straight to the product instead of leaving the reader
    /// to infer the connection from the title.
    pub review_product_name: Option<String>,
    pub review_product_slug: Option<String>,
    pub reply_count: i32,
    pub view_count: i32,
    pub is_pinned: bool,
    pub is_solved: bool,
    pub is_locked: bool,
    pub status: String,
    pub thumbnail_url: Option<String>,
    pub excerpt: Option<String>,
    pub last_post_at: Option<String>,
    pub created_at: String,
    pub tags: Vec<TagCtx>,
}

#[derive(Serialize, Clone)]
pub struct TagCtx {
    pub name: String,
    pub slug: String,
    pub color: Option<String>,
}

/// Category context for category pages.
#[derive(Serialize, Clone)]
pub struct CategoryCtx {
    pub id: String,
    pub parent_id: Option<String>,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub thread_count: u64,
    pub view_policy: String,
    pub post_policy: String,
    /// Mirrors PermissionChecker::can_create_post for the current viewer.
    /// Only meaningful where the template offers a "create thread here" action;
    /// listing-only contexts (search filter, mod queue filter, move-category
    /// picker) always set this to `false` since it's unused there.
    pub can_post: bool,
}

/// Forum index group: a parent category and its subcategories.
#[derive(Serialize, Clone)]
pub struct ForumGroupCtx {
    pub parent: CategoryCtx,
    pub children: Vec<CategoryCtx>,
    pub recent_threads: Vec<ThreadCtx>,
}

/// Post context for thread detail page.
#[derive(Serialize, Clone)]
pub struct PostCtx {
    pub id: String,
    pub author_username: String,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    pub author_trust_level: String,
    pub content_md: String,
    pub content_html: String,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub is_best_answer: bool,
    pub reactions: ReactionSummaryCtx,
    /// Soft-deleted — content_md/content_html are blanked server-side; template
    /// renders a "[deleted]" tombstone instead.
    pub is_deleted: bool,
    /// Awaiting moderator approval. Repository only returns another user's
    /// Pending posts to that user, so this is always the viewer's own post.
    pub is_pending: bool,
    /// Viewer is the post author.
    pub is_own: bool,
    /// Mirrors PermissionChecker::can_edit_post (edit window enforced server-side).
    pub can_edit: bool,
    /// Mirrors PermissionChecker::can_delete_post.
    pub can_delete: bool,
}

#[derive(Serialize, Clone)]
pub struct ReactionKindCtx {
    pub count: i64,
    pub reacted: bool,
}

#[derive(Serialize, Clone)]
pub struct ReactionSummaryCtx {
    pub like: ReactionKindCtx,
    pub helpful: ReactionKindCtx,
    pub insightful: ReactionKindCtx,
    pub funny: ReactionKindCtx,
}

impl Default for ReactionSummaryCtx {
    fn default() -> Self {
        let zero = ReactionKindCtx { count: 0, reacted: false };
        Self { like: zero.clone(), helpful: zero.clone(), insightful: zero.clone(), funny: zero }
    }
}

/// Thread detail context (includes posts).
#[derive(Serialize, Clone)]
pub struct ThreadDetailCtx {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub author_username: String,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    pub thumbnail_url: Option<String>,
    /// First ~160 chars of the opening post — used for the Open Graph/Twitter
    /// Card description, not rendered anywhere in the page body itself.
    pub excerpt: Option<String>,
    pub category_id: String,
    pub category_slug: String,
    pub category_name: String,
    /// Theme-facing only — no bundled template renders it.
    ///
    /// Page context is the API a user-installed theme is written against, and
    /// themes live outside this repository, so "no template reads it" cannot
    /// be established by searching. Tera renders a missing variable as an
    /// empty string rather than failing, so removing a field breaks such a
    /// theme silently, with nothing in the logs. Fields like this one are
    /// therefore kept deliberately, not by oversight.
    pub category_description: Option<String>,
    pub reply_count: i32,
    pub view_count: i32,
    pub is_pinned: bool,
    pub is_solved: bool,
    pub is_locked: bool,
    /// Drives the "Solved" badge's jump-to-post link — None even when
    /// is_solved is true if the best answer was later deleted.
    pub best_answer_id: Option<String>,
    pub created_at: String,
    pub tags: Vec<TagCtx>,
    pub posts: Vec<PostCtx>,
    pub pagination: PaginationCtx,
    pub can_pin: bool,
    pub can_lock: bool,
    pub can_move: bool,
    /// Mirrors ThreadUseCase::update_title (edit window enforced server-side).
    pub can_edit: bool,
    /// Mirrors ThreadUseCase::require_author_or_mod.
    pub can_delete: bool,
    /// Mirrors ThreadUseCase::mark_solved: author or thread.lock in category.
    pub can_mark_best_answer: bool,
}

/// Serializable Role for admin templates.
#[derive(Serialize, Clone)]
pub struct RoleCtx {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub color: Option<String>,
    pub is_system: bool,
    pub is_default: bool,
    pub member_count: u64,
}

/// Admin category list row — resolves parent_name and normalises policy strings.
#[derive(Serialize, Clone)]
pub struct AdminCategoryRowCtx {
    pub id: String,
    pub parent_id: Option<String>,
    pub parent_name: Option<String>,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub view_policy: String,
    pub post_policy: String,
    pub color: Option<String>,
    pub position: i32,
    pub thread_count: u64,
}

/// Serializable Permission for admin templates.
#[derive(Serialize, Clone)]
pub struct PermissionCtx {
    pub id: String,
    pub key: String,
    pub description: String,
    pub group_name: String,
    pub min_trust: String,
}

/// Serializable DashboardStats for the admin dashboard template.
#[derive(Serialize, Clone)]
pub struct DashboardStatsCtx {
    pub total_users: u64,
    pub total_threads: u64,
    pub total_posts: u64,
    pub pending_reports: u64,
    pub new_users_today: u64,
    pub new_threads_today: u64,
    pub new_posts_today: u64,
    pub new_reactions_today: u64,
    pub new_views_today: u64,
    pub dau: u64,
    pub mau: u64,
    /// DAU / MAU × 100, 1 decimal place.
    pub dau_mau_ratio: f64,
    /// % of users registered in the past 24h who posted today.
    pub activation_rate_pct: u64,
    pub oldest_pending_report_hours: Option<u64>,
}

/// Serializable product result for the search page. Mirrors the fields the
/// catalogue card macro reads, so search and browse render the identical card
/// rather than two that drift apart.
#[derive(Serialize, Clone)]
pub struct SearchProductCtx {
    pub slug: String,
    pub name: String,
    pub excerpt: Option<String>,
    pub product_type: String,
    pub style: Option<String>,
    pub primary_image_key: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    /// Theme-facing only — no bundled template renders it (the search page
    /// formats prices through the `thousands` filter without a currency).
    /// Kept because page context is the contract a user-installed theme is
    /// written against, and a cleanup pass has already flagged it once as
    /// unused; see the note on `ThreadDetailCtx::category_description`.
    pub currency: String,
    pub review_count: i32,
    pub avg_overall: Option<f64>,
}

/// Serializable search result hit for the search page.
#[derive(Serialize, Clone)]
pub struct SearchHitCtx {
    pub thread_slug: String,
    pub thread_title: String,
    pub excerpt: Option<String>,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    /// RFC 3339; None when the thread no longer resolves.
    pub created_at: Option<String>,
    pub reply_count: i32,
}

/// Notification item for the notifications inbox page.
#[derive(Serialize, Clone)]
pub struct NotificationCtx {
    pub id: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub is_read: bool,
    pub created_at: String,
}

/// Admin user list row — resolves optional fields to avoid Tera filter errors.
#[derive(Serialize, Clone)]
pub struct AdminUserRowCtx {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub trust_level: String,
    pub roles: Vec<RoleCtx>,
    pub is_banned: bool,
    pub created_at: String,
}

/// Admin user detail — includes resolved role list.
#[derive(Serialize, Clone)]
pub struct AdminUserDetailCtx {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub email_verified: bool,
    pub avatar_url: Option<String>,
    pub trust_level: String,
    pub trust_score: i32,
    pub roles: Vec<RoleCtx>,
    pub is_banned: bool,
    pub ban_reason: Option<String>,
    pub banned_until: Option<String>,
    pub warn_count: i32,
    pub failed_login_count: i32,
    pub locked_until: Option<String>,
    pub post_count: i32,
    pub days_visited: i32,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

/// Flat category entry for the left-sidebar navigation tree.
#[derive(Serialize, Clone)]
pub struct NavCategoryCtx {
    pub slug: String,
    pub name: String,
    pub color: Option<String>,
    pub children: Vec<NavCategoryCtx>,
}

/// One row of the homepage "latest reviews" panel.
///
/// Deliberately carries both the product and the reviewer: the product is what the
/// reader is deciding about, and the reviewer is what makes the panel read as a
/// living community rather than a feed of ratings. `product_slug` is `None` only
/// if the product row vanished between the two queries that build this — the
/// template falls back to linking the review thread itself.
#[derive(Serialize, Clone)]
pub struct LatestReviewCtx {
    /// Review thread slug — the link target, and the fallback when the product is gone.
    pub slug: String,
    pub product_name: Option<String>,
    pub product_slug: Option<String>,
    pub author_username: String,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    /// Overall score 1–5. `None` when the thread has no rating row yet.
    pub overall: Option<i16>,
    pub created_at: String,
}

/// User profile context.
#[derive(Serialize, Clone)]
pub struct UserProfileCtx {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub cover_url: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub trust_level: String,
    pub trust_score: Option<i32>,
    /// Theme-facing only — the bundled profile page styles the role badge from
    /// `primary_role_name`/`primary_role_color`, but a theme wanting to key CSS
    /// off the role needs the slug. See the note on
    /// `ThreadDetailCtx::category_description`.
    pub primary_role_slug: Option<String>,
    pub primary_role_name: Option<String>,
    pub primary_role_color: Option<String>,
    pub post_count: i32,
    pub follower_count: u64,
    pub following_count: u64,
    pub created_at: String,
    pub threads: Vec<ThreadCtx>,
    pub thread_pagination: Option<PaginationCtx>,
}

/// Account-page-only ban/warn status. Deliberately separate from `UserProfileCtx`
/// (which is also rendered on the public `/u/:username` page) so a ban reason
/// is never leaked to other visitors — only the account owner sees this.
#[derive(Serialize, Clone)]
pub struct AccountStatusCtx {
    pub is_banned: bool,
    pub ban_reason: Option<String>,
    /// None with is_banned=true means a permanent ban.
    pub banned_until: Option<String>,
    pub warn_count: i32,
    /// Drives the warning beside the email-notification toggles.
    ///
    /// `EventBus` refuses to mail an unverified address, so without this the page
    /// would show two switches turned on that silently deliver nothing — the exact
    /// "setting that appears to work and does not" failure the settings code warns
    /// about elsewhere. Lives here rather than on `CurrentUserCtx` because only
    /// this page needs it, and that struct is built for every page render.
    pub is_email_verified: bool,
}

/// Admin reports list row — enriched with reporter username and thread context.
#[derive(Serialize, Clone)]
pub struct AdminReportCtx {
    pub id: String,
    pub reporter_id: String,
    pub reporter_username: String,
    pub post_id: Option<String>,
    pub thread_id: Option<String>,
    pub thread_slug: Option<String>,
    pub thread_title: Option<String>,
    pub reason: String,
    pub status: String,
    pub created_at: String,
}

/// Audit log entry for mod/admin log pages.
#[derive(Serialize, Clone)]
pub struct AuditLogCtx {
    pub id: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    /// Human-readable label for the target (e.g. username for user targets).
    pub target_label: Option<String>,
    /// Navigable URL for the target entity, if resolvable.
    pub target_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// Plugin detail page — shown in /admin/plugins/{slug}.
#[derive(Serialize, Clone)]
pub struct PluginDetailCtx {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: String,
    pub status: String,
    pub is_active: bool,
    pub error_message: Option<String>,
    pub circuit_open: bool,
    /// JSON config as a pretty-printed string for the textarea editor.
    pub config_json: String,
    /// JSON config_schema from manifest (for rendering field hints).
    pub config_schema: serde_json::Value,
    pub install_path: String,
    pub installed_at: String,
    pub activated_at: Option<String>,
    pub logs: Vec<PluginLogCtx>,
}

/// Queue post item for mod/queue.html.
#[derive(Serialize, Clone)]
pub struct QueuePostCtx {
    pub id: String,
    pub author_username: String,
    pub thread_slug: String,
    pub thread_title: String,
    pub content_md: String,
    pub created_at: String,
}

/// Single plugin log entry.
#[derive(Serialize, Clone)]
pub struct PluginLogCtx {
    pub id: String,
    pub level: String,
    pub hook_name: Option<String>,
    pub duration_ms: Option<i32>,
    pub message: String,
    pub created_at: String,
}

