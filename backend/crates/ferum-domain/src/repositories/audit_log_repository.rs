use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::audit_log::AuditLog;
use crate::AppError;

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn append(&self, log: AuditLog) -> Result<(), AppError>;
    #[allow(clippy::too_many_arguments)]
    async fn list(
        &self,
        actor_id: Option<Uuid>,
        target_type: Option<&str>,
        action_contains: Option<&str>,
        created_from: Option<DateTime<Utc>>,
        created_to: Option<DateTime<Utc>>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<AuditLog>, u64), AppError>;
}
