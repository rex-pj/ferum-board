
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::thread::{Thread, ThreadStatus};
use crate::AppError;

/// How a thread listing is ORDERED (implements F-ORG-04).
///
/// This enum controls `ORDER BY` and **nothing else**. Narrowing the result set
/// is [`ThreadFeedFilter`]'s job, and the separation is load-bearing: these two
/// used to be one enum, which made them mutually exclusive in the URL and so
/// made "unanswered threads, newest first" — the single most useful query a
/// contributor can ask a forum — unexpressible. It also meant one tab strip
/// mixed controls that reorder the list with controls that make rows vanish,
/// under identical affordances.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThreadSort {
    /// Most recently active (last_post_at DESC). Default.
    #[default]
    Activity,
    /// By creation date (created_at DESC).
    Newest,
    /// By volume of discussion (reply_count DESC, then view_count DESC).
    ///
    /// Deliberately NOT called "hottest": there is no time decay here, so this
    /// ranks by all-time discussion volume, not by what is currently busy. The
    /// label follows the query rather than the other way round.
    MostReplies,
}

impl ThreadSort {
    /// Ordering alone, for callers with no filter axis (the admin thread list).
    /// Defined through [`parse_feed_query`] so the legacy vocabulary has exactly
    /// one definition.
    pub fn from_str(s: &str) -> Self {
        parse_feed_query(Some(s), None).0
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Newest => "newest",
            Self::MostReplies => "most_replies",
        }
    }
}

/// How a thread listing is NARROWED. Controls `WHERE`, never `ORDER BY`.
///
/// The two variants are mutually exclusive by construction, which matches the
/// data: a thread with zero replies cannot have a best answer, so `Unanswered`
/// and `Solved` can never both hold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThreadFeedFilter {
    /// No narrowing.
    #[default]
    All,
    /// Open threads nobody has replied to yet.
    Unanswered,
    /// Threads whose author or a moderator marked a best answer.
    Solved,
}

impl ThreadFeedFilter {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unanswered => "unanswered",
            Self::Solved => "solved",
        }
    }
}

/// Reads the `?sort=` / `?filter=` pair off a listing URL, accepting both the
/// current vocabulary and the pre-split one.
///
/// The third element is `true` when the input used the legacy vocabulary, so an
/// HTML handler can answer 301 with the canonical URL. Every legacy value is
/// still accepted forever — those URLs are bookmarked, linked from posts, and
/// indexed by search engines, and dropping them would fail silently: the request
/// would quietly fall back to the default listing with no error anywhere.
///
/// This lives in the domain, and is the ONLY place that knows the old spelling.
/// It is pure — no DB, no HTTP — which is what makes it cheap to test
/// exhaustively.
pub fn parse_feed_query(
    sort: Option<&str>,
    filter: Option<&str>,
) -> (ThreadSort, ThreadFeedFilter, bool) {
    let mut legacy = false;

    let (sort_from_legacy, filter_from_legacy) = match sort {
        // Pre-split spellings. The two that were really filters carry no sort of
        // their own, so they resolve to the default ordering they used to have.
        Some("latest") => {
            legacy = true;
            (Some(ThreadSort::Activity), None)
        }
        Some("hottest") => {
            legacy = true;
            (Some(ThreadSort::MostReplies), None)
        }
        Some("unanswered") => {
            legacy = true;
            (Some(ThreadSort::Activity), Some(ThreadFeedFilter::Unanswered))
        }
        Some("solved") => {
            legacy = true;
            (Some(ThreadSort::Activity), Some(ThreadFeedFilter::Solved))
        }
        _ => (None, None),
    };

    let resolved_sort = sort_from_legacy.unwrap_or(match sort {
        Some("newest") => ThreadSort::Newest,
        Some("most_replies") => ThreadSort::MostReplies,
        _ => ThreadSort::Activity,
    });

    // An explicit `?filter=` always wins over one implied by a legacy `?sort=`:
    // a caller writing the new vocabulary is stating intent, and mixing the two
    // is what a half-updated bookmark looks like.
    let resolved_filter = match filter {
        Some("unanswered") => ThreadFeedFilter::Unanswered,
        Some("solved") => ThreadFeedFilter::Solved,
        Some(_) | None => filter_from_legacy.unwrap_or_default(),
    };

    (resolved_sort, resolved_filter, legacy)
}

/// Filter bag passed to all thread list repository methods.
#[derive(Debug, Clone, Default)]
pub struct ThreadFilter {
    pub sort: ThreadSort,
    pub filter: ThreadFeedFilter,
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
    /// Batch fetch by ids, author-enriched. Order is not guaranteed — callers
    /// re-order as needed. Ids that don't resolve are simply absent.
    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<Thread>, AppError>;
    /// Author-enriched review threads for one product, newest first (non-deleted).
    async fn list_by_product(
        &self,
        product_id: Uuid,
        limit: u64,
    ) -> Result<Vec<Thread>, AppError>;
    /// The most recent review threads across all products, **at most one per
    /// product** (the newest), newest first.
    ///
    /// Backs the homepage "latest reviews" panel. The per-product cap is the point:
    /// a product that several people review in the same week would otherwise fill
    /// the panel by itself, and a five-row panel showing five different products
    /// carries five times the information. Note this is not a spam guard — a single
    /// account already cannot review one product twice (`uq_threads_product_author`,
    /// `uq_threads_product_author`), so the duplicates this collapses come from *different* people,
    /// which is exactly the signal the rating system wants. It is collapsed for
    /// display only; the product page still lists every review.
    ///
    /// Reviews of unpublished (draft) products are excluded, matching every other
    /// public listing.
    async fn list_latest_reviews(&self, limit: u64) -> Result<Vec<Thread>, AppError>;

    /// The author's existing (non-deleted) review of this product, if any.
    ///
    /// Backs the one-review-per-author rule. `uq_threads_product_author` enforces
    /// it in the database; this read exists so the use case can refuse with a
    /// useful message — and the slug of the review already written — instead of
    /// surfacing a unique-violation.
    async fn find_review_by_author(
        &self,
        product_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<Thread>, AppError>;
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
    /// Adds `delta` to view_count in a single UPDATE — used by the batch flush.
    async fn add_view_count(&self, id: Uuid, delta: i32) -> Result<(), AppError>;
    /// Records one thread view in the DB.
    /// - `viewer_key`: user_id (authenticated) or fingerprint hash (guest).
    /// - `viewer_type`: "user" | "guest".
    /// Returns `true` if this is the first view today → the caller must call increment_view_count.
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
    /// Set when the thread is a product review — links it to the reviewed product.
    pub product_id: Option<Uuid>,
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
