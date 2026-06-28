use std::sync::Arc;

use async_trait::async_trait;

use crate::ports::{ForumJob, JobQueue, NotificationBus, NullPluginRuntime, PluginHookRuntime};
use ferum_domain::events::ForumEvent;
use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::models::notification::NotificationKind;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_domain::repositories::webhook_repository::WebhookRepository;

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: ForumEvent);
}

pub struct EventBus {
    audit_log: Arc<dyn AuditLogRepository>,
    notifications: Arc<dyn NotificationRepository>,
    notification_bus: Arc<dyn NotificationBus>,
    webhooks: Arc<dyn WebhookRepository>,
    jobs: Arc<dyn JobQueue>,
    plugin_runtime: Arc<dyn PluginHookRuntime>,
}

impl EventBus {
    pub fn new(
        audit_log: Arc<dyn AuditLogRepository>,
        notifications: Arc<dyn NotificationRepository>,
        notification_bus: Arc<dyn NotificationBus>,
        webhooks: Arc<dyn WebhookRepository>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self {
            audit_log,
            notifications,
            notification_bus,
            webhooks,
            jobs,
            plugin_runtime: Arc::new(NullPluginRuntime),
        }
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginHookRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    async fn publish_inner(&self, event: ForumEvent) {
        tracing::info!(forum_event = event.event_type_str(), "event_bus: publishing");
        if let Err(e) = self.handle(&event).await {
            tracing::error!(
                forum_event = event.event_type_str(),
                error = ?e,
                "event_bus: handler failed"
            );
        }
        // Fire-and-forget plugin after-event dispatch (Tier 2/3 — Milestone 2)
        // Runs in background so it never blocks or affects the response.
        let event_type = event.event_type_str().to_string();
        let payload = serde_json::to_value(&event).unwrap_or_default();
        let runtime = self.plugin_runtime.clone();
        tokio::spawn(async move {
            runtime.dispatch_after_event(&event_type, payload).await;
        });
    }

    async fn handle(&self, event: &ForumEvent) -> Result<(), crate::shared::AppError> {
        match event {
            ForumEvent::PostDeleted {
                post_id,
                deleted_by_id,
            } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *deleted_by_id,
                        "post.deleted",
                        "post",
                        *post_id,
                        None,
                    ))
                    .await?;
                self.dispatch_webhooks("post.deleted", serde_json::json!({ "post_id": post_id }))
                    .await;
            }
            ForumEvent::ThreadLocked {
                thread_id,
                by_user_id,
            } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *by_user_id,
                        "thread.locked",
                        "thread",
                        *thread_id,
                        None,
                    ))
                    .await?;
                self.dispatch_webhooks(
                    "thread.locked",
                    serde_json::json!({ "thread_id": thread_id }),
                )
                .await;
            }
            ForumEvent::ThreadMoved {
                thread_id,
                by_user_id,
                from_category,
                to_category,
            } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *by_user_id,
                        "thread.moved",
                        "thread",
                        *thread_id,
                        Some(serde_json::json!({
                            "from_category": from_category,
                            "to_category": to_category,
                        })),
                    ))
                    .await?;
                self.dispatch_webhooks(
                    "thread.moved",
                    serde_json::json!({
                        "thread_id": thread_id,
                        "from_category": from_category,
                        "to_category": to_category,
                    }),
                )
                .await;
            }
            ForumEvent::UserBanned {
                user_id,
                by_user_id,
                reason,
                until,
            } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *by_user_id,
                        "user.banned",
                        "user",
                        *user_id,
                        Some(serde_json::json!({ "reason": reason, "until": until })),
                    ))
                    .await?;
                self.dispatch_webhooks(
                    "user.banned",
                    serde_json::json!({
                        "user_id": user_id, "reason": reason,
                    }),
                )
                .await;
            }
            ForumEvent::UserWarned {
                user_id,
                by_user_id,
                reason,
            } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *by_user_id,
                        "user.warned",
                        "user",
                        *user_id,
                        Some(serde_json::json!({ "reason": reason })),
                    ))
                    .await?;
                self.dispatch_webhooks(
                    "user.warned",
                    serde_json::json!({
                        "user_id": user_id, "reason": reason,
                    }),
                )
                .await;
            }
            ForumEvent::PostCreated {
                post_id,
                thread_id,
                thread_slug,
                thread_title,
                author_id,
                author_username,
                thread_author_id,
                ..
            } => {
                if author_id != thread_author_id {
                    let payload = serde_json::json!({
                        "kind": "reply",
                        "post_id": post_id,
                        "thread_id": thread_id,
                        "thread_slug": thread_slug,
                        "thread_title": thread_title,
                        "actor_username": author_username,
                    });
                    self.notifications
                        .create(*thread_author_id, NotificationKind::Reply, payload.clone())
                        .await
                        .ok();
                    self.notification_bus
                        .publish(*thread_author_id, payload)
                        .await
                        .ok();
                }
                self.dispatch_webhooks(
                    "post.created",
                    serde_json::json!({
                        "post_id": post_id,
                        "thread_id": thread_id,
                        "author_id": author_id,
                    }),
                )
                .await;
            }
            ForumEvent::ReactionAdded {
                post_id,
                thread_id,
                thread_slug,
                thread_title,
                post_author_id,
                reactor_id,
                reactor_username,
                kind,
            } => {
                if post_author_id != reactor_id {
                    let payload = serde_json::json!({
                        "kind": "reaction",
                        "post_id": post_id,
                        "thread_id": thread_id,
                        "thread_slug": thread_slug,
                        "thread_title": thread_title,
                        "actor_username": reactor_username,
                        "reaction": kind.as_str(),
                    });
                    self.notifications
                        .create(*post_author_id, NotificationKind::Reaction, payload.clone())
                        .await
                        .ok();
                    self.notification_bus
                        .publish(*post_author_id, payload)
                        .await
                        .ok();
                }
                self.dispatch_webhooks(
                    "reaction.added",
                    serde_json::json!({
                        "post_id": post_id,
                        "reactor_id": reactor_id,
                        "kind": kind.as_str(),
                    }),
                )
                .await;
            }
            ForumEvent::BestAnswerMarked {
                post_id,
                thread_id,
                thread_slug,
                thread_title,
                post_author_id,
                by_username,
                ..
            } => {
                let payload = serde_json::json!({
                    "kind": "best_answer",
                    "post_id": post_id,
                    "thread_id": thread_id,
                    "thread_slug": thread_slug,
                    "thread_title": thread_title,
                    "actor_username": by_username,
                });
                self.notifications
                    .create(
                        *post_author_id,
                        NotificationKind::BestAnswer,
                        payload.clone(),
                    )
                    .await
                    .ok();
                self.notification_bus
                    .publish(*post_author_id, payload)
                    .await
                    .ok();
                self.dispatch_webhooks(
                    "thread.best_answer_marked",
                    serde_json::json!({
                        "post_id": post_id,
                        "thread_id": thread_id,
                        "author_id": post_author_id,
                    }),
                )
                .await;
            }
            ForumEvent::MentionAdded {
                post_id,
                thread_id,
                thread_slug,
                thread_title,
                mentioned_user_id,
                author_id: _,
                author_username,
            } => {
                let payload = serde_json::json!({
                    "kind": "mention",
                    "post_id": post_id,
                    "thread_id": thread_id,
                    "thread_slug": thread_slug,
                    "thread_title": thread_title,
                    "actor_username": author_username,
                });
                self.notifications
                    .create(*mentioned_user_id, NotificationKind::Mention, payload.clone())
                    .await
                    .ok();
                self.notification_bus
                    .publish(*mentioned_user_id, payload)
                    .await
                    .ok();
            }
            ForumEvent::UserFollowed {
                follower_id,
                follower_username,
                followed_id,
            } => {
                let payload = serde_json::json!({
                    "kind": "follow",
                    "follower_id": follower_id,
                    "actor_username": follower_username,
                });
                self.notifications
                    .create(*followed_id, NotificationKind::System, payload.clone())
                    .await
                    .ok();
                self.notification_bus.publish(*followed_id, payload).await.ok();
                self.dispatch_webhooks(
                    "user.followed",
                    serde_json::json!({
                        "follower_id": follower_id,
                        "followed_id": followed_id,
                    }),
                )
                .await;
            }
            ForumEvent::ThreadCreated {
                thread_id,
                author_id,
                category_id,
                ..
            } => {
                self.dispatch_webhooks(
                    "thread.created",
                    serde_json::json!({
                        "thread_id": thread_id,
                        "author_id": author_id,
                        "category_id": category_id,
                    }),
                )
                .await;
            }
            ForumEvent::ThreadDeleted {
                thread_id,
                deleted_by_id,
            } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *deleted_by_id,
                        "thread.deleted",
                        "thread",
                        *thread_id,
                        None,
                    ))
                    .await?;
                self.dispatch_webhooks(
                    "thread.deleted",
                    serde_json::json!({ "thread_id": thread_id }),
                )
                .await;
            }
            _ => {}
        }
        Ok(())
    }

    async fn dispatch_webhooks(&self, event_type: &str, payload: serde_json::Value) {
        let hooks = match self.webhooks.find_subscribed(event_type).await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("webhook lookup failed for {}: {:?}", event_type, e);
                return;
            }
        };
        for hook in hooks {
            let job = ForumJob::SendWebhook {
                webhook_id: hook.id,
                url: hook.url,
                secret: hook.secret,
                event_type: event_type.to_string(),
                payload: payload.clone(),
            };
            self.jobs.enqueue(job).await.ok();
        }
    }
}

#[async_trait]
impl EventPublisher for EventBus {
    async fn publish(&self, event: ForumEvent) {
        self.publish_inner(event).await;
    }
}
