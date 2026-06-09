use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::view_models::user::UserSummaryResponse;
use ferum_domain::models::follow::Follow;
use ferum_domain::models::user::User;

#[derive(Serialize)]
pub struct FollowStatusResponse {
    pub following: bool,
    pub follower_count: u64,
    pub following_count: u64,
}

#[derive(Serialize)]
pub struct FollowResponse {
    pub id: Uuid,
    pub followed_at: DateTime<Utc>,
    pub user: UserSummaryResponse,
}

impl FollowResponse {
    pub fn from_pair(follow: Follow, user: User) -> Self {
        Self {
            id: follow.id,
            followed_at: follow.created_at,
            user: UserSummaryResponse::from(user),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FollowListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}
