use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub post_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub reason: String,
    pub status: ReportStatus,
    pub moderator_notes: Option<String>,
    pub resolved_by_id: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    /// Set when the reported content was hard-deleted; both post_id and thread_id will be None.
    pub target_deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum ReportStatus {
    Pending,
    Resolved,
    Dismissed,
}
