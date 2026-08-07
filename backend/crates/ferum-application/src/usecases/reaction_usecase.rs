use std::sync::Arc;

use uuid::Uuid;

use crate::event_bus::EventPublisher;
use crate::permission::PermissionChecker;
use crate::ports::{HookContext, HookDecision, NullPluginRuntime, PluginHookRuntime};
use crate::shared::{AppError, OptionExt};
use ferum_domain::events::ForumEvent;
use ferum_domain::models::post::Post;
use ferum_domain::models::reaction::ReactionKind;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;

pub struct ReactionUseCase {
    pub reactions: Arc<dyn ReactionRepository>,
    pub posts: Arc<dyn PostRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub users: Arc<dyn UserRepository>,
    pub event_bus: Arc<dyn EventPublisher>,
    pub plugin_runtime: Arc<dyn PluginHookRuntime>,
}

impl ReactionUseCase {
    pub fn new(
        reactions: Arc<dyn ReactionRepository>,
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        users: Arc<dyn UserRepository>,
        event_bus: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            reactions,
            posts,
            threads,
            users,
            event_bus,
            plugin_runtime: Arc::new(NullPluginRuntime),
        }
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginHookRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    async fn find_active_post(&self, post_id: Uuid) -> Result<Post, AppError> {
        let post = self
            .posts
            .find_by_id(post_id)
            .await?
            .or_not_found()?;
        if post.is_deleted {
            return Err(AppError::NotFound);
        }
        Ok(post)
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, post_id = %post_id, kind = ?kind))]
    pub async fn add(
        &self,
        actor: &AuthUser,
        post_id: Uuid,
        kind: ReactionKind,
    ) -> Result<Vec<(ReactionKind, u64)>, AppError> {
        PermissionChecker::can_react(actor)?;

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
            .or_not_found()?;

        let hook_ctx = HookContext {
            hook_name: "before_reaction_add".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "post_id": post_id,
                "thread_id": thread.id,
                "post_author_id": post.author_id,
                "kind": kind,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_reaction_add", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
        }

        self.reactions.add(post_id, actor.id, kind).await?;

        self.event_bus
            .publish(ForumEvent::ReactionAdded {
                post_id,
                thread_id: thread.id,
                thread_slug: thread.slug,
                thread_title: thread.title,
                post_author_id: post.author_id,
                reactor_id: actor.id,
                reactor_username: actor.username.clone(),
                kind,
            })
            .await;

        // Meaningful reactions (helpful, insightful) add 2 points to the post author's
        // trust_score. Like/funny reactions carry no score weight.
        // Fire-and-forget — never blocks the response.
        if matches!(kind, ReactionKind::Helpful | ReactionKind::Insightful) {
            let users = self.users.clone();
            let author_id = post.author_id;
            tokio::spawn(async move {
                let _ = users.increment_trust_score(author_id, 2).await;
            });
        }

        self.reactions.counts_by_post(post_id).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, post_id = %post_id, kind = ?kind))]
    pub async fn remove(
        &self,
        actor: &AuthUser,
        post_id: Uuid,
        kind: ReactionKind,
    ) -> Result<Vec<(ReactionKind, u64)>, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let post = self.find_active_post(post_id).await?;

        let existed = self.reactions.find(post_id, actor.id, kind).await?.is_some();
        self.reactions.remove(post_id, actor.id, kind).await?;

        if existed {
            self.event_bus
                .publish(ForumEvent::ReactionRemoved {
                    post_id,
                    post_author_id: post.author_id,
                    reactor_id: actor.id,
                    kind,
                })
                .await;

            // Undo the trust_score boost when a meaningful reaction is removed.
            if matches!(kind, ReactionKind::Helpful | ReactionKind::Insightful) {
                let users = self.users.clone();
                let author_id = post.author_id;
                tokio::spawn(async move {
                    let _ = users.increment_trust_score(author_id, -2).await;
                });
            }
        }

        self.reactions.counts_by_post(post_id).await
    }
}
