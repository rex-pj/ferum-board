use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::post::{Post, PostStatus};
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
use ferum_domain::AppError;

mockall::mock! {
    pub PostRepository {}

    #[async_trait]
    impl PostRepository for PostRepository {
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Post>, AppError>;
        async fn list_by_thread(&self, thread_id: Uuid, viewer_id: Option<Uuid>, page: u64, per_page: u64) -> Result<(Vec<Post>, u64), AppError>;
        async fn create(&self, cmd: NewPost) -> Result<Post, AppError>;
        async fn update_content(&self, id: Uuid, content_md: String, content_html: String, edited_by_id: Uuid) -> Result<Post, AppError>;
        async fn soft_delete(&self, id: Uuid, deleted_by_id: Uuid) -> Result<(), AppError>;
        async fn content_md_by_thread(&self, thread_id: Uuid) -> Result<Vec<String>, AppError>;
        async fn set_status(&self, id: Uuid, status: PostStatus) -> Result<(), AppError>;
        async fn list_by_author(&self, author_id: Uuid, page: u64, per_page: u64) -> Result<(Vec<Post>, u64), AppError>;
        async fn list_pending<'a>(&self, category_id: Option<Uuid>, allowed_category_ids: Option<&'a [Uuid]>, page: u64, per_page: u64) -> Result<(Vec<Post>, u64), AppError>;
        async fn position_in_thread(&self, thread_id: Uuid, post_id: Uuid, viewer_id: Option<Uuid>) -> Result<Option<u64>, AppError>;
    }
}
