use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_domain::AppError;

pub struct NoopAuditLogRepository;

#[async_trait]
impl AuditLogRepository for NoopAuditLogRepository {
    async fn append(&self, _log: AuditLog) -> Result<(), AppError> { Ok(()) }
    async fn list<'a>(&self, _actor_id: Option<Uuid>, _target_type: Option<&'a str>, _action_contains: Option<&'a str>, _created_from: Option<DateTime<Utc>>, _created_to: Option<DateTime<Utc>>, _page: u64, _per_page: u64) -> Result<(Vec<AuditLog>, u64), AppError> {
        Ok((vec![], 0))
    }
}
