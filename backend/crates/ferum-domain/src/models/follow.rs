use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Follow {
    pub id: Uuid,
    pub follower_id: Uuid,
    pub followed_id: Uuid,
    pub created_at: DateTime<Utc>,
}
