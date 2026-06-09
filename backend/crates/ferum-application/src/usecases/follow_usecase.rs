use std::sync::Arc;

use uuid::Uuid;

use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::follow::Follow;
use ferum_domain::models::user::User;
use ferum_domain::repositories::follow_repository::FollowRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;

pub struct FollowUseCase {
    pub follows: Arc<dyn FollowRepository>,
    pub users: Arc<dyn UserRepository>,
    pub event_bus: Arc<EventBus>,
}

impl FollowUseCase {
    pub fn new(
        follows: Arc<dyn FollowRepository>,
        users: Arc<dyn UserRepository>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self { follows, users, event_bus }
    }

    pub async fn follow(&self, actor: &AuthUser, target_user_id: Uuid) -> Result<bool, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        if actor.id == target_user_id {
            return Err(AppError::UnprocessableEntity(
                "You cannot follow yourself.".to_string(),
            ));
        }

        let target = self
            .users
            .find_by_id(target_user_id)
            .await?
            .ok_or(AppError::NotFound)?;

        if self.follows.find(actor.id, target_user_id).await?.is_some() {
            return Ok(true);
        }

        self.follows.add(actor.id, target_user_id).await?;

        let follower_username = actor.username.clone();
        self.event_bus
            .publish(ForumEvent::UserFollowed {
                follower_id: actor.id,
                follower_username,
                followed_id: target.id,
            })
            .await;

        Ok(true)
    }

    pub async fn unfollow(
        &self,
        actor: &AuthUser,
        target_user_id: Uuid,
    ) -> Result<bool, AppError> {
        PermissionChecker::require_not_banned(actor)?;
        self.follows.remove(actor.id, target_user_id).await?;
        Ok(false)
    }

    pub async fn get_status(
        &self,
        viewer_id: Option<Uuid>,
        target_user_id: Uuid,
    ) -> Result<FollowStatus, AppError> {
        let following = match viewer_id {
            Some(id) => self.follows.find(id, target_user_id).await?.is_some(),
            None => false,
        };
        let follower_count = self.follows.count_followers(target_user_id).await?;
        let following_count = self.follows.count_following(target_user_id).await?;
        Ok(FollowStatus {
            following,
            follower_count,
            following_count,
        })
    }

    pub async fn list_following(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError> {
        self.follows.list_following(user_id, page, per_page).await
    }

    pub async fn list_followers(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError> {
        self.follows.list_followers(user_id, page, per_page).await
    }
}

pub struct FollowStatus {
    pub following: bool,
    pub follower_count: u64,
    pub following_count: u64,
}
