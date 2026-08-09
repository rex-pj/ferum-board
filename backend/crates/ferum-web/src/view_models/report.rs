use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::report::Report;

#[derive(Debug, Deserialize)]
pub struct ReportListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub status: Option<String>,
    pub target_type: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub actor_id: Option<Uuid>,
    pub target_type: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateReportRequest {
    pub post_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    #[validate(length(min = 1, max = 500))] // ferum_application::constants::MAX_REPORT_REASON_LEN
    pub reason: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResolveReportRequest {
    #[validate(custom(function = "crate::view_models::validators::valid_report_status"))]
    pub status: String,
    #[validate(length(max = 2000, message = "Moderator notes must be at most 2000 characters"))]
    pub moderator_notes: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct WarnUserRequest {
    #[validate(length(min = 1, max = 1000))]
    pub reason: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TempBanRequest {
    #[validate(length(min = 1, max = 1000))]
    pub reason: String,
    pub until: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ReportResponse {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub post_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub reason: String,
    pub status: String,
    pub moderator_notes: Option<String>,
    pub resolved_by_id: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    /// Non-null when the reported content was hard-deleted; post_id and thread_id will both be null.
    pub target_deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Report> for ReportResponse {
    fn from(r: Report) -> Self {
        Self {
            id: r.id,
            reporter_id: r.reporter_id,
            post_id: r.post_id,
            thread_id: r.thread_id,
            reason: r.reason,
            status: format!("{:?}", r.status).to_lowercase(),
            moderator_notes: r.moderator_notes,
            resolved_by_id: r.resolved_by_id,
            resolved_at: r.resolved_at,
            target_deleted_at: r.target_deleted_at,
            created_at: r.created_at,
        }
    }
}
