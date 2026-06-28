use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::category::Category;
use ferum_domain::repositories::category_repository::{
    CategoryRepository, NewCategory, UpdateCategory,
};
use ferum_domain::AppError;

mockall::mock! {
    pub CategoryRepository {}

    #[async_trait]
    impl CategoryRepository for CategoryRepository {
        async fn list_all(&self) -> Result<Vec<Category>, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Category>, AppError>;
        async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Category>, AppError>;
        async fn create(&self, cmd: NewCategory) -> Result<Category, AppError>;
        async fn update(&self, id: Uuid, patch: UpdateCategory) -> Result<Category, AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;
        async fn has_children(&self, id: Uuid) -> Result<bool, AppError>;
        async fn count_threads_by_categories(&self, ids: &[Uuid]) -> Result<Vec<(Uuid, u64)>, AppError>;
    }
}
