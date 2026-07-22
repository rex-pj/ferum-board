use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::material::{Material, NewMaterial, UpdateMaterial};
use ferum_domain::repositories::material_repository::MaterialRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub MaterialRepository {}

    #[async_trait]
    impl MaterialRepository for MaterialRepository {
        async fn create(&self, material: NewMaterial) -> Result<Material, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Material>, AppError>;
        async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Material>, AppError>;
        async fn list<'a>(&self, category: Option<&'a str>) -> Result<Vec<Material>, AppError>;
        async fn update(&self, id: Uuid, patch: UpdateMaterial) -> Result<Material, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
    }
}
