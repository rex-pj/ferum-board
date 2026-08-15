use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub position: i32,
    pub view_policy: ViewPolicy,
    pub post_policy: PostPolicy,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

/// Who may see a category exists. Enforced by `PermissionChecker`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum ViewPolicy {
    Public,
    /// Basic trust; a guest gets 401, so existence is not hidden.
    MembersOnly,
    /// Refused with **404, not 403** — a 403 would confirm the category exists
    /// to an unauthorised reader.
    StaffOnly,
}

/// Minimum trust to post into a category. Does not grant permission — RBAC
/// still applies — and staff with moderation rights here bypass the gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum PostPolicy {
    /// Basic trust and above.
    Members,
    /// Member trust and above.
    Trusted,
    /// Leader trust, which is only ever assigned by hand.
    StaffOnly,
    /// Nobody, staff included. Checked before the permission and trust gates.
    Closed,
    /// Same trust floor as [`Members`](PostPolicy::Members) — the difference
    /// lands after acceptance, where `PostUseCase` marks the post `pending`.
    /// Gating it harder would reject the authors the queue exists to catch.
    Moderated,
}
