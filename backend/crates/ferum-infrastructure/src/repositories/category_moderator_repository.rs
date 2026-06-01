use async_trait::async_trait;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::category_moderators;
use ferum_application::shared::AppError;
use ferum_domain::models::category::CategoryModerator;
use ferum_domain::repositories::category_moderator_repository::CategoryModeratorRepository;

pub struct PgCategoryModeratorRepository {
    db: DatabaseConnection,
}

impl PgCategoryModeratorRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: category_moderators::Model) -> CategoryModerator {
    CategoryModerator {
        id: m.id,
        category_id: m.category_id,
        user_id: m.user_id,
        assigned_at: m.assigned_at.with_timezone(&chrono::Utc),
        assigned_by_id: m.assigned_by_id,
    }
}

#[async_trait]
impl CategoryModeratorRepository for PgCategoryModeratorRepository {
    async fn list_by_category(
        &self,
        category_id: Uuid,
    ) -> Result<Vec<CategoryModerator>, AppError> {
        let rows = category_moderators::Entity::find()
            .filter(category_moderators::Column::CategoryId.eq(category_id))
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(entity_to_domain).collect())
    }

    async fn list_category_ids_for_user(&self, user_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let rows = category_moderators::Entity::find()
            .filter(category_moderators::Column::UserId.eq(user_id))
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(|r| r.category_id).collect())
    }

    async fn assign(
        &self,
        category_id: Uuid,
        user_id: Uuid,
        assigned_by_id: Uuid,
    ) -> Result<CategoryModerator, AppError> {
        let model = category_moderators::ActiveModel {
            id: Set(uuid::Uuid::new_v4()),
            category_id: Set(category_id),
            user_id: Set(user_id),
            assigned_by_id: Set(Some(assigned_by_id)),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted))
    }

    async fn revoke(&self, category_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        category_moderators::Entity::delete_many()
            .filter(category_moderators::Column::CategoryId.eq(category_id))
            .filter(category_moderators::Column::UserId.eq(user_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn is_assigned(&self, category_id: Uuid, user_id: Uuid) -> Result<bool, AppError> {
        let count = category_moderators::Entity::find()
            .filter(category_moderators::Column::CategoryId.eq(category_id))
            .filter(category_moderators::Column::UserId.eq(user_id))
            .count(&self.db)
            .await?;
        Ok(count > 0)
    }
}
