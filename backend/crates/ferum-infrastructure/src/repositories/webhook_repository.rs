use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::webhooks;
use ferum_application::constants::WEBHOOK_MAX_FAILURES;
use ferum_application::shared::AppError;
use ferum_domain::models::webhook::Webhook;
use ferum_domain::repositories::webhook_repository::{
    NewWebhook, UpdateWebhook, WebhookRepository,
};

pub struct PgWebhookRepository {
    db: DatabaseConnection,
}

impl PgWebhookRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: webhooks::Model) -> Webhook {
    Webhook {
        id: m.id,
        url: m.url,
        events: m.events,
        secret: m.secret,
        is_active: m.is_active,
        created_at: m.created_at.with_timezone(&Utc),
        updated_at: m.updated_at.map(|t| t.with_timezone(&Utc)),
        created_by_id: m.created_by_id,
        last_triggered_at: m.last_triggered_at.map(|t| t.with_timezone(&Utc)),
        failure_count: m.failure_count,
        plugin_id: m.plugin_id,
    }
}

#[async_trait]
impl WebhookRepository for PgWebhookRepository {
    async fn list(&self) -> Result<Vec<Webhook>, AppError> {
        Ok(webhooks::Entity::find()
            .order_by_asc(webhooks::Column::CreatedAt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(entity_to_domain)
            .collect())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, AppError> {
        Ok(webhooks::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn find_subscribed(&self, event_type: &str) -> Result<Vec<Webhook>, AppError> {
        Ok(webhooks::Entity::find()
            .filter(webhooks::Column::IsActive.eq(true))
            .filter(Expr::cust_with_values(
                "$1 = ANY(events)",
                [event_type.to_string()],
            ))
            .all(&self.db)
            .await?
            .into_iter()
            .map(entity_to_domain)
            .collect())
    }

    async fn create(&self, new: NewWebhook) -> Result<Webhook, AppError> {
        let model = webhooks::ActiveModel {
            id: Set(Uuid::new_v4()),
            url: Set(new.url),
            events: Set(new.events),
            secret: Set(new.secret),
            is_active: Set(true),
            created_by_id: Set(new.created_by_id),
            plugin_id: Set(new.plugin_id),
            ..Default::default()
        };
        Ok(entity_to_domain(model.insert(&self.db).await?))
    }

    async fn update(&self, id: Uuid, upd: UpdateWebhook) -> Result<Webhook, AppError> {
        let existing = webhooks::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        let mut active: webhooks::ActiveModel = existing.into();
        if let Some(url) = upd.url {
            active.url = Set(url);
        }
        if let Some(events) = upd.events {
            active.events = Set(events);
        }
        if let Some(secret) = upd.secret {
            active.secret = Set(Some(secret));
        }
        if let Some(is_active) = upd.is_active {
            active.is_active = Set(is_active);
        }

        Ok(entity_to_domain(active.update(&self.db).await?))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    async fn delete_by_plugin(&self, plugin_id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::delete_many()
            .filter(webhooks::Column::PluginId.eq(plugin_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn record_success(&self, id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::update_many()
            .col_expr(
                webhooks::Column::LastTriggeredAt,
                Expr::value(Utc::now().fixed_offset()),
            )
            .col_expr(webhooks::Column::FailureCount, Expr::value(0i32))
            .filter(webhooks::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn record_failure(&self, id: Uuid) -> Result<(), AppError> {
        // Increment failure_count atomically
        self.db
            .execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE webhooks SET failure_count = failure_count + 1 WHERE id = $1",
                [id.into()],
            ))
            .await?;

        // Auto-disable if threshold reached
        webhooks::Entity::update_many()
            .col_expr(webhooks::Column::IsActive, Expr::value(false))
            .filter(webhooks::Column::Id.eq(id))
            .filter(webhooks::Column::FailureCount.gte(WEBHOOK_MAX_FAILURES))
            .exec(&self.db)
            .await?;

        Ok(())
    }
}
