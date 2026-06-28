use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::tag::{NewTag, Tag};
use crate::AppError;

#[async_trait]
pub trait TagRepository: Send + Sync {
    async fn list<'a>(&self, query: Option<&'a str>, limit: u32) -> Result<Vec<Tag>, AppError>;
    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Tag>, AppError>;
    async fn create(&self, tag: NewTag) -> Result<Tag, AppError>;
    async fn assign_to_thread(&self, thread_id: Uuid, tag_ids: &[Uuid]) -> Result<(), AppError>;
    async fn replace_thread_tags(&self, thread_id: Uuid, tag_ids: &[Uuid]) -> Result<(), AppError>;
    async fn find_by_thread(&self, thread_id: Uuid) -> Result<Vec<Tag>, AppError>;
    async fn find_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<Tag>>, AppError>;
}
