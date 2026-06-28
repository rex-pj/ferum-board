use async_trait::async_trait;
use uuid::Uuid;

use crate::models::report::{Report, ReportStatus};
use crate::AppError;

pub struct ReportStatusCounts {
    pub pending: u64,
    pub resolved: u64,
    pub dismissed: u64,
}

#[async_trait]
pub trait ReportRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Report>, AppError>;
    async fn count_by_status(&self) -> Result<ReportStatusCounts, AppError>;
    async fn list_all<'a>(
        &self,
        status: Option<ReportStatus>,
        target_type: Option<&'a str>,
        q: Option<&'a str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError>;
    async fn create(
        &self,
        reporter_id: Uuid,
        post_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        reason: String,
    ) -> Result<Report, AppError>;
    async fn update_status(
        &self,
        id: Uuid,
        status: ReportStatus,
        resolved_by_id: Uuid,
        moderator_notes: Option<String>,
    ) -> Result<(), AppError>;
}
