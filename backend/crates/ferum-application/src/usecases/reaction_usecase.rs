use std::sync::Arc;

use uuid::Uuid;

use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::post::Post;
use ferum_domain::models::reaction::ReactionKind;
use ferum_domain::models::user::TrustLevel;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::AuthUser;

pub struct ReactionUseCase {
    pub reactions: Arc<dyn ReactionRepository>,
    pub posts: Arc<dyn PostRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub event_bus: Arc<EventBus>,
}

impl ReactionUseCase {
    pub fn new(
        reactions: Arc<dyn ReactionRepository>,
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            reactions,
            posts,
            threads,
            event_bus,
        }
    }

    async fn find_active_post(&self, post_id: Uuid) -> Result<Post, AppError> {
        let post = self
            .posts
            .find_by_id(post_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if post.is_deleted {
            return Err(AppError::NotFound);
        }
        Ok(post)
    }

    pub async fn add(
        &self,
        actor: &AuthUser,
        post_id: Uuid,
        kind: ReactionKind,
    ) -> Result<Vec<(ReactionKind, u64)>, AppError> {
        PermissionChecker::require_not_banned(actor)?;
        if actor.trust_level < TrustLevel::Basic {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }

        let post = self.find_active_post(post_id).await?;

        if post.author_id == actor.id {
            return Err(AppError::forbidden("cannot_react_to_own_post"));
        }

        // Idempotent: if already exists, return current counts
        if self
            .reactions
            .find(post_id, actor.id, kind)
            .await?
            .is_some()
        {
            return self.reactions.counts_by_post(post_id).await;
        }

        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .ok_or(AppError::NotFound)?;

        self.reactions.add(post_id, actor.id, kind).await?;

        self.event_bus
            .publish(ForumEvent::ReactionAdded {
                post_id,
                thread_id: thread.id,
                thread_slug: thread.slug,
                post_author_id: post.author_id,
                reactor_id: actor.id,
                kind,
            })
            .await;

        self.reactions.counts_by_post(post_id).await
    }

    pub async fn remove(
        &self,
        actor: &AuthUser,
        post_id: Uuid,
        kind: ReactionKind,
    ) -> Result<Vec<(ReactionKind, u64)>, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let post = self.find_active_post(post_id).await?;

        self.reactions.remove(post_id, actor.id, kind).await?;

        self.event_bus
            .publish(ForumEvent::ReactionRemoved {
                post_id,
                post_author_id: post.author_id,
                reactor_id: actor.id,
                kind,
            })
            .await;

        self.reactions.counts_by_post(post_id).await
    }
}
