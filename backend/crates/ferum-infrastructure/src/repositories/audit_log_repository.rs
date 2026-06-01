use async_trait::async_trait;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::audit_logs;
use ferum_application::shared::AppError;
use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;

pub struct PgAuditLogRepository {
    db: DatabaseConnection,
}

impl PgAuditLogRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: audit_logs::Model) -> AuditLog {
    AuditLog {
        id: m.id,
        actor_id: m.actor_id,
        action: m.action,
        target_type: m.target_type,
        target_id: m.target_id,
        metadata: m.metadata,
        created_at: m.created_at.with_timezone(&chrono::Utc),
    }
}

#[async_trait]
impl AuditLogRepository for PgAuditLogRepository {
    async fn append(&self, log: AuditLog) -> Result<(), AppError> {
        let model = audit_logs::ActiveModel {
            id: Set(log.id),
            actor_id: Set(log.actor_id),
            action: Set(log.action),
            target_type: Set(log.target_type),
            target_id: Set(log.target_id),
            metadata: Set(log.metadata),
            created_at: Set(log.created_at.fixed_offset()),
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn list(
        &self,
        actor_id: Option<Uuid>,
        target_type: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<AuditLog>, u64), AppError> {
        let mut query = audit_logs::Entity::find();

        if let Some(id) = actor_id {
            query = query.filter(audit_logs::Column::ActorId.eq(id));
        }
        if let Some(tt) = target_type {
            query = query.filter(audit_logs::Column::TargetType.eq(tt));
        }

        let total = query.clone().count(&self.db).await?;
        let offset = (page.saturating_sub(1)) * per_page;

        let rows = query
            .order_by_desc(audit_logs::Column::CreatedAt)
            .limit(per_page)
            .offset(offset)
            .all(&self.db)
            .await?;

        Ok((rows.into_iter().map(entity_to_domain).collect(), total))
    }
}
