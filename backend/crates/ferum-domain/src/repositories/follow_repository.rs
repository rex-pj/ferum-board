use async_trait::async_trait;
use uuid::Uuid;

use crate::models::follow::Follow;
use crate::models::user::User;
use crate::AppError;

#[async_trait]
pub trait FollowRepository: Send + Sync {
    async fn find(&self, follower_id: Uuid, followed_id: Uuid) -> Result<Option<Follow>, AppError>;
    async fn add(&self, follower_id: Uuid, followed_id: Uuid) -> Result<Follow, AppError>;
    async fn remove(&self, follower_id: Uuid, followed_id: Uuid) -> Result<(), AppError>;
    async fn list_following(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError>;
    async fn list_followers(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError>;
    async fn count_following(&self, user_id: Uuid) -> Result<u64, AppError>;
    async fn count_followers(&self, user_id: Uuid) -> Result<u64, AppError>;
}
