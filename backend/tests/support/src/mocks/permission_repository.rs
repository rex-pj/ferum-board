use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::role::Permission;
use ferum_domain::repositories::permission_repository::PermissionRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub PermissionRepository {}

    #[async_trait]
    impl PermissionRepository for PermissionRepository {
        async fn list_all(&self) -> Result<Vec<Permission>, AppError>;
        async fn list_for_role(&self, role_id: Uuid) -> Result<Vec<Permission>, AppError>;
        async fn set_role_permissions(&self, role_id: Uuid, permission_keys: &[String]) -> Result<(), AppError>;
    }
}
