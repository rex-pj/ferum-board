use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::constants::MAX_BAN_REASON_LEN;
use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::ports::CacheService;
use crate::shared::AppError;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::report::{Report, ReportStatus};
use ferum_domain::models::user::UserRole;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::report_repository::ReportRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::repositories::user_repository::{UpdateUser, UserRepository};
use ferum_domain::AuthUser;

pub struct ModerationUseCase {
    pub reports: Arc<dyn ReportRepository>,
    pub posts: Arc<dyn PostRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub users: Arc<dyn UserRepository>,
    pub notifications: Arc<dyn NotificationRepository>,
    pub audit_log_repo: Arc<dyn AuditLogRepository>,
    pub event_bus: Arc<EventBus>,
    pub cache: Arc<dyn CacheService>,
}

impl ModerationUseCase {
    pub fn new(
        reports: Arc<dyn ReportRepository>,
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        users: Arc<dyn UserRepository>,
        notifications: Arc<dyn NotificationRepository>,
        audit_log_repo: Arc<dyn AuditLogRepository>,
        event_bus: Arc<EventBus>,
        cache: Arc<dyn CacheService>,
    ) -> Self {
        Self {
            reports,
            posts,
            threads,
            users,
            notifications,
            audit_log_repo,
            event_bus,
            cache,
        }
    }

    // ─── Report creation (any authenticated member) ───────────────────────────

    pub async fn create_report(
        &self,
        actor: &AuthUser,
        cmd: CreateReportCmd,
    ) -> Result<Report, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        if cmd.post_id.is_none() && cmd.thread_id.is_none() {
            return Err(AppError::unprocessable("post_id or thread_id is required"));
        }

        if let Some(post_id) = cmd.post_id {
            self.posts
                .find_by_id(post_id)
                .await?
                .ok_or(AppError::NotFound)?;
        }
        if let Some(thread_id) = cmd.thread_id {
            self.threads
                .find_by_id(thread_id)
                .await?
                .ok_or(AppError::NotFound)?;
        }

        self.reports
            .create(actor.id, cmd.post_id, cmd.thread_id, cmd.reason)
            .await
    }

    // ─── Report queue (moderator) ─────────────────────────────────────────────

    pub async fn list_pending_reports(
        &self,
        actor: &AuthUser,
        target_type: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        PermissionChecker::can_moderate_any(actor)
            .or_else(|_| PermissionChecker::can_admin(actor))?;
        self.reports
            .list_all(
                Some(ReportStatus::Pending),
                target_type,
                page,
                per_page.min(50),
            )
            .await
    }

    pub async fn list_all_reports(
        &self,
        actor: &AuthUser,
        status: Option<ReportStatus>,
        target_type: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        PermissionChecker::can_admin(actor)?;
        self.reports
            .list_all(status, target_type, page, per_page.min(50))
            .await
    }

    pub async fn resolve_report(
        &self,
        actor: &AuthUser,
        report_id: Uuid,
        status: ReportStatus,
        moderator_notes: Option<String>,
    ) -> Result<(), AppError> {
        PermissionChecker::can_moderate_any(actor)
            .or_else(|_| PermissionChecker::can_admin(actor))?;

        self.reports
            .find_by_id(report_id)
            .await?
            .ok_or(AppError::NotFound)?;
        self.reports
            .update_status(report_id, status, actor.id, moderator_notes)
            .await
    }

    // ─── User actions (moderator) ─────────────────────────────────────────────

    pub async fn warn_user(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
        reason: String,
    ) -> Result<(), AppError> {
        if reason.len() > MAX_BAN_REASON_LEN {
            return Err(AppError::unprocessable(
                "Reason must be 1 000 characters or fewer",
            ));
        }
        PermissionChecker::can_moderate_any(actor)
            .or_else(|_| PermissionChecker::can_admin(actor))?;

        let target_user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if target_user.role >= actor.role && actor.role < UserRole::Admin {
            return Err(AppError::forbidden("cannot_warn_higher_or_equal_role"));
        }

        self.users
            .update(
                user_id,
                UpdateUser {
                    warn_count_delta: Some(1),
                    ..Default::default()
                },
            )
            .await?;

        // Notify the user
        self.notifications
            .create(
                user_id,
                ferum_domain::models::notification::NotificationKind::Warn,
                serde_json::json!({ "reason": reason, "by": actor.username }),
            )
            .await?;

        self.event_bus
            .publish(ForumEvent::UserWarned {
                user_id,
                by_user_id: actor.id,
                reason,
            })
            .await;

        Ok(())
    }

    pub async fn temp_ban(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
        reason: String,
        until: DateTime<Utc>,
    ) -> Result<(), AppError> {
        if reason.len() > MAX_BAN_REASON_LEN {
            return Err(AppError::unprocessable(
                "Reason must be 1 000 characters or fewer",
            ));
        }
        PermissionChecker::can_moderate_any(actor)
            .or_else(|_| PermissionChecker::can_admin(actor))?;

        let target = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if target.role >= actor.role {
            return Err(AppError::forbidden("cannot_ban_higher_or_equal_role"));
        }

        self.users
            .update(
                user_id,
                UpdateUser {
                    is_banned: Some(true),
                    banned_until: Some(Some(until)),
                    ban_reason: Some(Some(reason.clone())),
                    ..Default::default()
                },
            )
            .await?;

        // Set immediate-ban cache key so middleware rejects existing tokens instantly.
        // Also revoke all refresh tokens so the user cannot re-authenticate.
        let ttl = (until - Utc::now())
            .to_std()
            .unwrap_or(Duration::from_secs(0));
        if !ttl.is_zero() {
            self.cache
                .set(&format!("user:banned:{}", user_id), "1", ttl)
                .await
                .ok();
        }
        self.cache
            .del_prefix(&format!("refresh:{}:", user_id))
            .await
            .ok();

        self.event_bus
            .publish(ForumEvent::UserBanned {
                user_id,
                by_user_id: actor.id,
                reason,
                until: Some(until),
            })
            .await;

        Ok(())
    }
}

#[derive(Debug)]
pub struct CreateReportCmd {
    pub post_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub reason: String,
}
