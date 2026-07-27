use async_trait::async_trait;
use uuid::Uuid;

use crate::models::post::{Post, PostStatus};
use crate::AppError;

#[async_trait]
pub trait PostRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Post>, AppError>;
    /// Batch counterpart to [`find_by_id`], for callers holding a page of ids.
    /// Mirrors `ThreadRepository::find_many_by_ids` / `UserRepository::find_many_by_ids`:
    /// no ordering guarantee and missing ids are simply absent, so callers must
    /// index the result by id rather than by position.
    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<Post>, AppError>;
    /// `viewer_id`, when set, also includes that user's own `Pending` posts
    /// (so an author sees their own queued post) — `Pending` posts by anyone
    /// else are still excluded. Soft-deleted posts are always included (as
    /// tombstones the caller renders as "[deleted]") so reply threads don't
    /// lose context when a parent is removed.
    async fn list_by_thread(
        &self,
        thread_id: Uuid,
        viewer_id: Option<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError>;
    async fn create(&self, cmd: NewPost) -> Result<Post, AppError>;
    async fn update_content(
        &self,
        id: Uuid,
        content_md: String,
        content_html: String,
        edited_by_id: Uuid,
    ) -> Result<Post, AppError>;
    async fn soft_delete(&self, id: Uuid, deleted_by_id: Uuid) -> Result<(), AppError>;
    /// `content_md` of every *non-soft-deleted* post in the thread.
    ///
    /// Used at thread deletion to release the attachment references those posts
    /// hold. Already-deleted posts are excluded because their own delete path
    /// released their references; including them would decrement twice and
    /// un-publish an image another post still embeds.
    ///
    /// Returns one entry per post, not a deduplicated set: two posts embedding
    /// the same image hold two references, and both must be released.
    async fn content_md_by_thread(&self, thread_id: Uuid) -> Result<Vec<String>, AppError>;
    async fn set_status(&self, id: Uuid, status: PostStatus) -> Result<(), AppError>;
    async fn list_by_author(
        &self,
        author_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError>;
    /// `category_id`: optional caller-supplied filter (a single category).
    /// `allowed_category_ids`: `None` = unrestricted (global moderator/admin);
    /// `Some(ids)` = hard restriction to the categories the actor moderates.
    /// The two are ANDed, so a category-scoped moderator can never see pending
    /// posts outside their assigned categories regardless of the filter they pass.
    async fn list_pending<'a>(
        &self,
        category_id: Option<Uuid>,
        allowed_category_ids: Option<&'a [Uuid]>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError>;
    /// 0-based position of `post_id` within the same ordered/filtered result
    /// set `list_by_thread` would return for this `viewer_id` — lets a caller
    /// compute which page a post (e.g. a notification target, a best answer)
    /// lands on. `None` if the post doesn't exist or isn't visible to viewer_id.
    async fn position_in_thread(
        &self,
        thread_id: Uuid,
        post_id: Uuid,
        viewer_id: Option<Uuid>,
    ) -> Result<Option<u64>, AppError>;
}

#[derive(Debug, Clone)]
pub struct NewPost {
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
    pub content_html: String,
    pub status: PostStatus,
}
