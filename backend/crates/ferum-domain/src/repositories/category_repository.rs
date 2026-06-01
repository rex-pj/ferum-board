use async_trait::async_trait;
use uuid::Uuid;

use crate::models::category::{Category, PostPolicy, ViewPolicy};
use crate::AppError;

#[async_trait]
pub trait CategoryRepository: Send + Sync {
    async fn list_all(&self) -> Result<Vec<Category>, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Category>, AppError>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Category>, AppError>;
    async fn create(&self, cmd: NewCategory) -> Result<Category, AppError>;
    async fn update(&self, id: Uuid, patch: UpdateCategory) -> Result<Category, AppError>;
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;
    /// Returns `(category_id, thread_count)` pairs for the given IDs.
    /// Only counts non-deleted threads.
    async fn count_threads_by_categories(&self, ids: &[Uuid])
        -> Result<Vec<(Uuid, u64)>, AppError>;
}

#[derive(Debug, Clone)]
pub struct NewCategory {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub view_policy: ViewPolicy,
    pub post_policy: PostPolicy,
    pub color: Option<String>,
    pub created_by_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateCategory {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<Option<String>>,
    pub parent_id: Option<Option<Uuid>>,
    pub position: Option<i32>,
    pub view_policy: Option<ViewPolicy>,
    pub post_policy: Option<PostPolicy>,
    pub color: Option<Option<String>>,
    pub updated_by_id: Option<Uuid>,
}
