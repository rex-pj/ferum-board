use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::role::Role;
use ferum_domain::repositories::role_repository::{NewRole, RoleRepository, UpdateRole};
use ferum_domain::AppError;

mockall::mock! {
    pub RoleRepository {}

    #[async_trait]
    impl RoleRepository for RoleRepository {
        async fn list(&self) -> Result<Vec<Role>, AppError>;
        async fn list_default(&self) -> Result<Vec<Role>, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Role>, AppError>;
        async fn find_by_slug(&self, slug: &str) -> Result<Option<Role>, AppError>;
        async fn create(&self, new: NewRole) -> Result<Role, AppError>;
        async fn update(&self, id: Uuid, patch: UpdateRole) -> Result<Role, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
    }
}
