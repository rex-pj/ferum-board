#![allow(dead_code)]

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Bookmark {
    pub id: Uuid,
    pub user_id: Uuid,
    pub thread_id: Uuid,
    pub created_at: DateTime<Utc>,
}
