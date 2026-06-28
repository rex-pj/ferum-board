use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::bookmark::Bookmark;
use ferum_domain::models::thread::Thread;
use ferum_domain::repositories::bookmark_repository::BookmarkRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub BookmarkRepository {}

    #[async_trait]
    impl BookmarkRepository for BookmarkRepository {
        async fn find(&self, user_id: Uuid, thread_id: Uuid) -> Result<Option<Bookmark>, AppError>;
        async fn list_for_user(&self, user_id: Uuid, page: u64, per_page: u64) -> Result<(Vec<(Bookmark, Thread)>, u64), AppError>;
        async fn add(&self, user_id: Uuid, thread_id: Uuid) -> Result<Bookmark, AppError>;
        async fn remove(&self, user_id: Uuid, thread_id: Uuid) -> Result<(), AppError>;
    }
}
