use async_trait::async_trait;
use uuid::Uuid;

use crate::models::audit_log::AuditLog;
use crate::AppError;

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn append(&self, log: AuditLog) -> Result<(), AppError>;
    async fn list(
        &self,
        actor_id: Option<Uuid>,
        target_type: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<AuditLog>, u64), AppError>;
}
