use async_trait::async_trait;
use uuid::Uuid;

use crate::models::role::Role;
use crate::AppError;

pub struct NewRole {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub position: i32,
}

pub struct UpdateRole {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub color: Option<Option<String>>,
    pub position: Option<i32>,
}

#[async_trait]
pub trait RoleRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Role>, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Role>, AppError>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Role>, AppError>;
    async fn create(&self, new: NewRole) -> Result<Role, AppError>;
    async fn update(&self, id: Uuid, patch: UpdateRole) -> Result<Role, AppError>;
    /// Returns error if role is_system = true.
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;
}
