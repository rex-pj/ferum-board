use serde::Serialize;

use crate::middleware::AuthUser;

/// A single rendered plugin Web Component element for a UI slot.
/// Pre-rendered as HTML so Tera can output it with `{{ slot.html | safe }}`.
#[derive(Serialize, Clone)]
pub struct PluginSlotCtx {
    /// Ready-to-render HTML, e.g. `<my-plugin-widget data-foo="bar"></my-plugin-widget>`
    pub html: String,
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
}

impl CurrentUserCtx {
    pub fn from_auth(u: &AuthUser, unread_count: u64) -> Self {
        let is_admin = u.has_perm("admin.users");
        let is_moderator = u.has_perm("moderation.view_reports") || is_admin;
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
        }
    }
}

impl From<&AuthUser> for CurrentUserCtx {
    fn from(u: &AuthUser) -> Self {
        Self::from_auth(u, 0)
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
        let total_pages = (total + per_page - 1) / per_page.max(1);
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
    pub category_id: String,
    pub category_slug: String,
    pub category_name: String,
    pub category_description: Option<String>,
    pub reply_count: i32,
    pub view_count: i32,
    pub is_pinned: bool,
    pub is_solved: bool,
    pub is_locked: bool,
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

