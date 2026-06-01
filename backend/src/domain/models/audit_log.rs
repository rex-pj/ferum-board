use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub target_type: String,
    pub target_id: Uuid,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl AuditLog {
    pub fn user_action(
        actor_id: Uuid,
        action: impl Into<String>,
        target_type: impl Into<String>,
        target_id: Uuid,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            actor_id: Some(actor_id),
            action: action.into(),
            target_type: target_type.into(),
            target_id,
            metadata,
            created_at: Utc::now(),
        }
    }

}
