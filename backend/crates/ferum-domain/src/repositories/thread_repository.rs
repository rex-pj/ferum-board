
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::thread::{Thread, ThreadStatus};
use crate::AppError;

/// Sort/filter preset for thread listing pages (implements F-ORG-04).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ThreadSort {
    /// Most recently active (pinned first, then last_post_at DESC). Default.
    #[default]
    Latest,
    /// By creation date (pinned first, then created_at DESC).
    Newest,
    /// By engagement (pinned first, then reply_count DESC).
    Hottest,
    /// Open threads with no replies (reply_count = 0, created_at DESC).
    Unanswered,
    /// Threads marked as solved (is_solved = true, last_post_at DESC).
    Solved,
}

impl ThreadSort {
    pub fn from_str(s: &str) -> Self {
        match s {
            "newest" => Self::Newest,
            "hottest" => Self::Hottest,
            "unanswered" => Self::Unanswered,
            "solved" => Self::Solved,
            _ => Self::Latest,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Latest => "latest",
            Self::Newest => "newest",
            Self::Hottest => "hottest",
            Self::Unanswered => "unanswered",
            Self::Solved => "solved",
        }
    }
}

/// Filter bag passed to all thread list repository methods.
#[derive(Debug, Clone, Default)]
pub struct ThreadFilter {
    pub sort: ThreadSort,
}

/// Filter bag for the admin thread listing page. All fields are optional;
/// `None` means "no constraint on this dimension".
#[derive(Debug, Clone, Default)]
pub struct AdminThreadFilter {
    /// Full-text search against the thread title.
    pub search: Option<String>,
    /// Status keyword: "open" | "locked" | "deleted". `None` = all non-deleted.
    pub status: Option<String>,
    /// Restrict to a single category.
    pub category_id: Option<Uuid>,
    /// Restrict to a single author.
    pub author_id: Option<Uuid>,
    /// Inclusive lower bound on created_at.
    pub created_from: Option<DateTime<Utc>>,
    /// Inclusive upper bound on created_at.
    pub created_to: Option<DateTime<Utc>>,
    pub sort: ThreadSort,
}

#[async_trait]
pub trait ThreadRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Thread>, AppError>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Thread>, AppError>;
    /// When `cached_total` is `Some`, the COUNT query is skipped and the supplied
    /// value is returned as the total — used by the use-case layer to avoid an exact
    /// COUNT(*) on every paginated guest request.
    async fn list_by_category(
        &self,
        category_id: Uuid,
        filter: &ThreadFilter,
        page: u64,
        per_page: u64,
        cached_total: Option<u64>,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    /// See [`Self::list_by_category`] for the meaning of `cached_total`.
    async fn list_feed(
        &self,
        category_ids: &[Uuid],
        filter: &ThreadFilter,
        page: u64,
        per_page: u64,
        cached_total: Option<u64>,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    async fn list_by_author(
        &self,
        author_id: Uuid,
        category_ids: &[Uuid],
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    async fn list_by_tag(
        &self,
        tag_slug: &str,
        category_ids: &[Uuid],
        filter: &ThreadFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    async fn create(&self, cmd: NewThread) -> Result<Thread, AppError>;
    async fn update(&self, id: Uuid, patch: UpdateThread) -> Result<Thread, AppError>;
    /// Thêm `delta` vào view_count trong một UPDATE duy nhất — dùng bởi batch flush.
    async fn add_view_count(&self, id: Uuid, delta: i32) -> Result<(), AppError>;
    /// Ghi nhận một lần xem thread vào DB.
    /// - `viewer_key`: user_id (authenticated) hoặc fingerprint hash (guest).
    /// - `viewer_type`: "user" | "guest".
    /// Trả về `true` nếu đây là lần xem đầu tiên hôm nay → caller phải gọi increment_view_count.
    async fn try_record_view(
        &self,
        thread_id: Uuid,
        viewer_key: &str,
        viewer_type: &str,
    ) -> Result<bool, AppError>;
    /// `last_post_at: None` leaves the column untouched — used when the change (e.g. a
    /// post deletion) does not represent new latest activity on the thread.
    async fn update_reply_stats(
        &self,
        id: Uuid,
        reply_count_delta: i32,
        last_post_at: Option<DateTime<Utc>>,
    ) -> Result<(), AppError>;
    async fn set_thumbnail(&self, thread_id: Uuid, file_key: String) -> Result<(), AppError>;
    async fn remove_thumbnail(&self, thread_id: Uuid) -> Result<(), AppError>;
    async fn find_thumbnail_key(&self, thread_id: Uuid) -> Result<Option<String>, AppError>;
    /// Returns up to `limit_per_category` most-recent threads for each of the given
    /// category IDs in a single query (window function). Results are ordered by
    /// category_id then pinned-first / last_post_at desc.
    async fn list_recent_by_categories(
        &self,
        category_ids: &[Uuid],
        limit_per_category: u64,
    ) -> Result<Vec<Thread>, AppError>;

    /// Admin-only listing: searches across ALL categories and ALL statuses
    /// (including deleted). Supports title search, status/category/author/date
    /// filters, and sort — see [`AdminThreadFilter`].
    async fn list_admin_threads(
        &self,
        filter: &AdminThreadFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError>;
}

#[derive(Debug, Clone)]
pub struct NewThread {
    pub id: Uuid,
    pub category_id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    pub slug: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateThread {
    pub title: Option<String>,
    pub status: Option<ThreadStatus>,
    pub is_pinned: Option<bool>,
    pub is_solved: Option<bool>,
    pub best_answer_id: Option<Option<Uuid>>,
    pub category_id: Option<Uuid>,
    pub deleted_by_id: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
}
