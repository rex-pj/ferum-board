use std::sync::Arc;

use crate::application::ports::{ForumJob, JobQueue, NotificationBus};
use crate::domain::events::ForumEvent;
use crate::domain::models::audit_log::AuditLog;
use crate::domain::models::notification::NotificationKind;
use crate::domain::repositories::audit_log_repository::AuditLogRepository;
use crate::domain::repositories::notification_repository::NotificationRepository;
use crate::domain::repositories::webhook_repository::WebhookRepository;

pub struct EventBus {
    audit_log: Arc<dyn AuditLogRepository>,
    notifications: Arc<dyn NotificationRepository>,
    notification_bus: Arc<dyn NotificationBus>,
    webhooks: Arc<dyn WebhookRepository>,
    jobs: Arc<dyn JobQueue>,
}

impl EventBus {
    pub fn new(
        audit_log: Arc<dyn AuditLogRepository>,
        notifications: Arc<dyn NotificationRepository>,
        notification_bus: Arc<dyn NotificationBus>,
        webhooks: Arc<dyn WebhookRepository>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self { audit_log, notifications, notification_bus, webhooks, jobs }
    }

    pub async fn publish(&self, event: ForumEvent) {
        if let Err(e) = self.handle(&event).await {
            tracing::error!("event bus error for {:?}: {:?}", std::mem::discriminant(&event), e);
        }
    }

    async fn handle(&self, event: &ForumEvent) -> Result<(), crate::application::shared::AppError> {
        match event {
            ForumEvent::PostDeleted { post_id, deleted_by_id } => {
                self.audit_log
                    .append(AuditLog::user_action(*deleted_by_id, "post.deleted", "post", *post_id, None))
                    .await?;
                self.dispatch_webhooks("post.deleted", serde_json::json!({ "post_id": post_id })).await;
            }
            ForumEvent::ThreadLocked { thread_id, by_user_id } => {
                self.audit_log
                    .append(AuditLog::user_action(*by_user_id, "thread.locked", "thread", *thread_id, None))
                    .await?;
                self.dispatch_webhooks("thread.locked", serde_json::json!({ "thread_id": thread_id })).await;
            }
            ForumEvent::ThreadMoved { thread_id, by_user_id, from_category, to_category } => {
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
                self.dispatch_webhooks("thread.moved", serde_json::json!({
                    "thread_id": thread_id,
                    "from_category": from_category,
                    "to_category": to_category,
                })).await;
            }
            ForumEvent::UserBanned { user_id, by_user_id, reason, until } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *by_user_id,
                        "user.banned",
                        "user",
                        *user_id,
                        Some(serde_json::json!({ "reason": reason, "until": until })),
                    ))
                    .await?;
                self.dispatch_webhooks("user.banned", serde_json::json!({
                    "user_id": user_id, "reason": reason,
                })).await;
            }
            ForumEvent::UserWarned { user_id, by_user_id, reason } => {
                self.audit_log
                    .append(AuditLog::user_action(
                        *by_user_id,
                        "user.warned",
                        "user",
                        *user_id,
                        Some(serde_json::json!({ "reason": reason })),
                    ))
                    .await?;
                self.dispatch_webhooks("user.warned", serde_json::json!({
                    "user_id": user_id, "reason": reason,
                })).await;
            }
            ForumEvent::PostCreated { post_id, thread_id, thread_slug, author_id, thread_author_id, .. } => {
                if author_id != thread_author_id {
                    let payload = serde_json::json!({
                        "kind": "reply",
                        "post_id": post_id,
                        "thread_id": thread_id,
                        "thread_slug": thread_slug,
                        "author_id": author_id,
                    });
                    self.notifications.create(*thread_author_id, NotificationKind::Reply, payload.clone()).await.ok();
                    self.notification_bus.publish(*thread_author_id, payload).await.ok();
                }
                self.dispatch_webhooks("post.created", serde_json::json!({
                    "post_id": post_id,
                    "thread_id": thread_id,
                    "author_id": author_id,
                })).await;
            }
            ForumEvent::ReactionAdded { post_id, thread_id, thread_slug, post_author_id, reactor_id, kind } => {
                if post_author_id != reactor_id {
                    let payload = serde_json::json!({
                        "kind": "reaction",
                        "post_id": post_id,
                        "thread_id": thread_id,
                        "thread_slug": thread_slug,
                        "reactor_id": reactor_id,
                        "reaction": kind.as_str(),
                    });
                    self.notifications.create(*post_author_id, NotificationKind::Reaction, payload.clone()).await.ok();
                    self.notification_bus.publish(*post_author_id, payload).await.ok();
                }
                self.dispatch_webhooks("reaction.added", serde_json::json!({
                    "post_id": post_id,
                    "reactor_id": reactor_id,
                    "kind": kind.as_str(),
                })).await;
            }
            ForumEvent::BestAnswerMarked { post_id, thread_id, thread_slug, post_author_id, .. } => {
                let payload = serde_json::json!({
                    "kind": "best_answer",
                    "post_id": post_id,
                    "thread_id": thread_id,
                    "thread_slug": thread_slug,
                });
                self.notifications.create(*post_author_id, NotificationKind::BestAnswer, payload.clone()).await.ok();
                self.notification_bus.publish(*post_author_id, payload).await.ok();
                self.dispatch_webhooks("thread.best_answer_marked", serde_json::json!({
                    "post_id": post_id,
                    "thread_id": thread_id,
                    "author_id": post_author_id,
                })).await;
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
