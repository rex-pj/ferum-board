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
    /// `category_ids`: `None` = unrestricted (global admin/mod); `Some(ids)` = only
    /// count reports whose target (post's thread, or thread) belongs to one of `ids`.
    async fn count_by_status<'a>(
        &self,
        category_ids: Option<&'a [Uuid]>,
    ) -> Result<ReportStatusCounts, AppError>;
    /// `category_ids`: `None` = unrestricted (global admin/mod); `Some(ids)` = only
    /// list reports whose target (post's thread, or thread) belongs to one of `ids`.
    /// Used to enforce category-scoped moderator visibility — a moderator assigned
    /// to category A must not see or act on reports filed in category B.
    async fn list_all<'a>(
        &self,
        status: Option<ReportStatus>,
        target_type: Option<&'a str>,
        q: Option<&'a str>,
        category_ids: Option<&'a [Uuid]>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError>;
    /// Reports filed BY this user (not against them) — lets a reporter track
    /// their own submissions without needing moderator access.
    async fn list_by_reporter(
        &self,
        reporter_id: Uuid,
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
