use async_trait::async_trait;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::notifications;
use ferum_application::shared::AppError;
use ferum_domain::models::notification::{Notification, NotificationKind};
use ferum_domain::repositories::notification_repository::NotificationRepository;

pub struct PgNotificationRepository {
    db: DatabaseConnection,
}

impl PgNotificationRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: notifications::Model) -> Notification {
    Notification {
        id: m.id,
        user_id: m.user_id,
        kind: match m.kind {
            notifications::NotificationKind::Reply => NotificationKind::Reply,
            notifications::NotificationKind::Mention => NotificationKind::Mention,
            notifications::NotificationKind::Reaction => NotificationKind::Reaction,
            notifications::NotificationKind::BestAnswer => NotificationKind::BestAnswer,
            notifications::NotificationKind::Warn => NotificationKind::Warn,
            notifications::NotificationKind::System => NotificationKind::System,
        },
        payload: m.payload,
        is_read: m.is_read,
        read_at: m.read_at.map(|t| t.with_timezone(&chrono::Utc)),
        created_at: m.created_at.with_timezone(&chrono::Utc),
    }
}

fn kind_to_entity(k: &NotificationKind) -> notifications::NotificationKind {
    match k {
        NotificationKind::Reply => notifications::NotificationKind::Reply,
        NotificationKind::Mention => notifications::NotificationKind::Mention,
        NotificationKind::Reaction => notifications::NotificationKind::Reaction,
        NotificationKind::BestAnswer => notifications::NotificationKind::BestAnswer,
        NotificationKind::Warn => notifications::NotificationKind::Warn,
        NotificationKind::System => notifications::NotificationKind::System,
    }
}

#[async_trait]
impl NotificationRepository for PgNotificationRepository {
    async fn list_for_user(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Notification>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;
        let query = notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user_id))
            .order_by_desc(notifications::Column::CreatedAt);

        let (total, rows) = tokio::try_join!(
            query.clone().count(&self.db),
            query.limit(per_page).offset(offset).all(&self.db),
        )?;
        Ok((rows.into_iter().map(entity_to_domain).collect(), total))
    }

    async fn unread_count(&self, user_id: Uuid) -> Result<u64, AppError> {
        Ok(notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user_id))
            .filter(notifications::Column::IsRead.eq(false))
            .count(&self.db)
            .await?)
    }

    async fn create(
        &self,
        user_id: Uuid,
        kind: NotificationKind,
        payload: serde_json::Value,
    ) -> Result<Notification, AppError> {
        let model = notifications::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            kind: Set(kind_to_entity(&kind)),
            payload: Set(payload),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted))
    }

    async fn mark_read(&self, id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        notifications::Entity::update_many()
            .col_expr(notifications::Column::IsRead, Expr::value(true))
            .col_expr(
                notifications::Column::ReadAt,
                Expr::value(chrono::Utc::now().fixed_offset()),
            )
            .filter(notifications::Column::Id.eq(id))
            .filter(notifications::Column::UserId.eq(user_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn mark_all_read(&self, user_id: Uuid) -> Result<(), AppError> {
        notifications::Entity::update_many()
            .col_expr(notifications::Column::IsRead, Expr::value(true))
            .col_expr(
                notifications::Column::ReadAt,
                Expr::value(chrono::Utc::now().fixed_offset()),
            )
            .filter(notifications::Column::UserId.eq(user_id))
            .filter(notifications::Column::IsRead.eq(false))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
