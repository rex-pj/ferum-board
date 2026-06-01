#![allow(dead_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::thread::{Thread, ThreadStatus};
use crate::AppError;

#[async_trait]
pub trait ThreadRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Thread>, AppError>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Thread>, AppError>;
    async fn list_by_category(
        &self,
        category_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    async fn list_feed(
        &self,
        category_ids: &[Uuid],
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    async fn list_by_author(
        &self,
        author_id: Uuid,
        category_ids: &[Uuid],
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError>;
    async fn create(&self, cmd: NewThread) -> Result<Thread, AppError>;
    async fn update(&self, id: Uuid, patch: UpdateThread) -> Result<Thread, AppError>;
    async fn increment_view_count(&self, id: Uuid) -> Result<(), AppError>;
    async fn update_reply_stats(
        &self,
        id: Uuid,
        reply_count_delta: i32,
        last_post_at: DateTime<Utc>,
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
