use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ferum_domain::models::thread::Thread;
use ferum_domain::repositories::thread_repository::{
    AdminThreadFilter, NewThread, ThreadFilter, ThreadRepository, UpdateThread,
};
use ferum_domain::AppError;

mockall::mock! {
    pub ThreadRepository {}

    #[async_trait]
    impl ThreadRepository for ThreadRepository {
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Thread>, AppError>;
        async fn find_by_slug(&self, slug: &str) -> Result<Option<Thread>, AppError>;
        async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<Thread>, AppError>;
        async fn list_by_category(&self, category_id: Uuid, filter: &ThreadFilter, page: u64, per_page: u64, cached_total: Option<u64>) -> Result<(Vec<Thread>, u64), AppError>;
        async fn list_feed(&self, category_ids: &[Uuid], filter: &ThreadFilter, page: u64, per_page: u64, cached_total: Option<u64>) -> Result<(Vec<Thread>, u64), AppError>;
        async fn list_by_author(&self, author_id: Uuid, category_ids: &[Uuid], page: u64, per_page: u64) -> Result<(Vec<Thread>, u64), AppError>;
        async fn list_by_tag(&self, tag_slug: &str, category_ids: &[Uuid], filter: &ThreadFilter, page: u64, per_page: u64) -> Result<(Vec<Thread>, u64), AppError>;
        async fn create(&self, cmd: NewThread) -> Result<Thread, AppError>;
        async fn update(&self, id: Uuid, patch: UpdateThread) -> Result<Thread, AppError>;
        async fn add_view_count(&self, id: Uuid, delta: i32) -> Result<(), AppError>;
        async fn try_record_view(&self, thread_id: Uuid, viewer_key: &str, viewer_type: &str) -> Result<bool, AppError>;
        async fn update_reply_stats(&self, id: Uuid, reply_count_delta: i32, last_post_at: Option<DateTime<Utc>>) -> Result<(), AppError>;
        async fn set_thumbnail(&self, thread_id: Uuid, file_key: String) -> Result<(), AppError>;
        async fn remove_thumbnail(&self, thread_id: Uuid) -> Result<(), AppError>;
        async fn find_thumbnail_key(&self, thread_id: Uuid) -> Result<Option<String>, AppError>;
        async fn list_recent_by_categories(&self, category_ids: &[Uuid], limit_per_category: u64) -> Result<Vec<Thread>, AppError>;
        async fn list_admin_threads(&self, filter: &AdminThreadFilter, page: u64, per_page: u64) -> Result<(Vec<Thread>, u64), AppError>;
    }
}
