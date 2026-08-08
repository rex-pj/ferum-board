use std::sync::Arc;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::shared::{AppError, OptionExt};
use ferum_domain::models::bookmark::Bookmark;
use ferum_domain::models::thread::Thread;
use ferum_domain::repositories::bookmark_repository::BookmarkRepository;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::AuthUser;

pub struct BookmarkUseCase {
    pub bookmarks: Arc<dyn BookmarkRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    /// So `add` can check the thread's category is one the actor may see.
    /// Without it, a bookmark on a `staff_only` thread was accepted and the
    /// thread's title then rendered on the bookmarks page — the one place a
    /// restricted title could reach a reader who had never been allowed to open
    /// it.
    pub categories: Arc<dyn CategoryRepository>,
}

impl BookmarkUseCase {
    pub fn new(
        bookmarks: Arc<dyn BookmarkRepository>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
    ) -> Self {
        Self { bookmarks, threads, categories }
    }

    pub async fn is_bookmarked(&self, user_id: Uuid, thread_id: Uuid) -> Result<bool, AppError> {
        Ok(self.bookmarks.find(user_id, thread_id).await?.is_some())
    }

    pub async fn add(&self, actor: &AuthUser, thread_id: Uuid) -> Result<bool, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let thread = self
            .threads
            .find_by_id(thread_id)
            .await?
            .or_not_found()?;
        if matches!(
            thread.status,
            ferum_domain::models::thread::ThreadStatus::Deleted
        ) {
            return Err(AppError::NotFound);
        }

        // Checked before the idempotency shortcut so a hidden category answers
        // 404 either way — otherwise the two answers differ and that difference
        // reports whether the thread exists.
        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .or_not_found()?;
        PermissionChecker::can_view_category(Some(actor), &category)?;

        if self.bookmarks.find(actor.id, thread_id).await?.is_some() {
            return Ok(true);
        }

        self.bookmarks.add(actor.id, thread_id).await?;
        Ok(true)
    }

    pub async fn remove(&self, actor: &AuthUser, thread_id: Uuid) -> Result<bool, AppError> {
        PermissionChecker::require_not_banned(actor)?;
        self.bookmarks.remove(actor.id, thread_id).await?;
        Ok(false)
    }

    pub async fn list(
        &self,
        actor: &AuthUser,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Bookmark, Thread)>, u64), AppError> {
        PermissionChecker::require_not_banned(actor)?;
        self.bookmarks.list_for_user(actor.id, page, per_page.min(crate::constants::MAX_LIST_PAGE_SIZE)).await
    }
}
