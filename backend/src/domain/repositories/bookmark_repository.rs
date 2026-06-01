use async_trait::async_trait;
use uuid::Uuid;

use crate::application::shared::AppError;
use crate::domain::models::bookmark::Bookmark;
use crate::domain::models::thread::Thread;

#[async_trait]
pub trait BookmarkRepository: Send + Sync {
    async fn find(&self, user_id: Uuid, thread_id: Uuid) -> Result<Option<Bookmark>, AppError>;
    async fn list_for_user(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Bookmark, Thread)>, u64), AppError>;
    async fn add(&self, user_id: Uuid, thread_id: Uuid) -> Result<Bookmark, AppError>;
    async fn remove(&self, user_id: Uuid, thread_id: Uuid) -> Result<(), AppError>;
}
