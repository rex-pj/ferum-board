use std::sync::Arc;

use uuid::Uuid;

use crate::event_bus::EventPublisher;
use crate::permission::PermissionChecker;
use crate::ports::{HookContext, HookDecision, NullPluginRuntime, PluginHookRuntime};
use crate::shared::{AppError, OptionExt};
use ferum_domain::events::ForumEvent;
use ferum_domain::models::post::Post;
use ferum_domain::models::reaction::ReactionKind;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;

pub struct ReactionUseCase {
    pub reactions: Arc<dyn ReactionRepository>,
    pub posts: Arc<dyn PostRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    /// Needed to answer "may this actor see where they are reacting?". Reacting
    /// is a write into a category and it notifies the post's author, so it is
    /// subject to the category's `view_policy` like every other write — but the
    /// path resolved the post and stopped, so it was the one write that was not.
    pub categories: Arc<dyn CategoryRepository>,
    pub users: Arc<dyn UserRepository>,
    pub event_bus: Arc<dyn EventPublisher>,
    pub plugin_runtime: Arc<dyn PluginHookRuntime>,
}

impl ReactionUseCase {
    pub fn new(
        reactions: Arc<dyn ReactionRepository>,
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        users: Arc<dyn UserRepository>,
        event_bus: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            reactions,
            posts,
            threads,
            categories,
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

    /// The post's thread, plus the check that `actor` may see where it lives.
    ///
    /// Returns 404 rather than 403 for a hidden category, which
    /// `can_view_category` already does for `staff_only` — a category whose
    /// existence is not admitted must not be confirmed by a reaction endpoint
    /// either (NF-SC-13).
    async fn visible_thread(
        &self,
        actor: &AuthUser,
        post: &Post,
    ) -> Result<ferum_domain::models::thread::Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .or_not_found()?;
        if thread.deleted_at.is_some() {
            return Err(AppError::NotFound);
        }
        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .or_not_found()?;
        PermissionChecker::can_view_category(Some(actor), &category)?;
        Ok(thread)
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

        // Before the idempotency shortcut: an actor who may not see the category
        // must get the same 404 whether or not they already reacted, or the
        // difference between the two answers reveals that the post exists.
        let thread = self.visible_thread(actor, &post).await?;

        // Idempotent: if already exists, return current counts
        if self
            .reactions
            .find(post_id, actor.id, kind)
            .await?
            .is_some()
        {
            return self.reactions.counts_by_post(post_id).await;
        }

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
        // Same gate as `add`. Withdrawing a reaction is a smaller act than
        // making one, but it still writes into the category and still tells the
        // caller the post is there.
        self.visible_thread(actor, &post).await?;

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
