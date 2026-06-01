use async_trait::async_trait;
use uuid::Uuid;

use crate::models::post::Post;
use crate::AppError;

#[async_trait]
pub trait PostRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Post>, AppError>;
    async fn list_by_thread(
        &self,
        thread_id: Uuid,
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
}

#[derive(Debug, Clone)]
pub struct NewPost {
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
    pub content_html: String,
}
