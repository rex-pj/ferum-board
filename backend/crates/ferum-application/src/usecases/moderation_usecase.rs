use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::constants::MAX_BAN_REASON_LEN;
use crate::dto::ReportWithContext;
use crate::event_bus::EventPublisher;
use crate::permission::PermissionChecker;
use crate::ports::{CacheService, HookContext, HookDecision, NullPluginRuntime, PluginHookRuntime};
use crate::shared::{AppError, OptionExt};
use ferum_domain::events::ForumEvent;
use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::models::report::{Report, ReportStatus};

use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::report_repository::{ReportRepository, ReportStatusCounts};
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
    pub event_bus: Arc<dyn EventPublisher>,
    pub cache: Arc<dyn CacheService>,
    pub plugin_runtime: Arc<dyn PluginHookRuntime>,
}

impl ModerationUseCase {
    pub fn new(
        reports: Arc<dyn ReportRepository>,
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        users: Arc<dyn UserRepository>,
        notifications: Arc<dyn NotificationRepository>,
        audit_log_repo: Arc<dyn AuditLogRepository>,
        event_bus: Arc<dyn EventPublisher>,
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
            plugin_runtime: Arc::new(NullPluginRuntime),
        }
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginHookRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    // ─── Report creation (any authenticated member) ───────────────────────────

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id))]
    pub async fn create_report(
        &self,
        actor: &AuthUser,
        cmd: CreateReportCmd,
    ) -> Result<Report, AppError> {
        PermissionChecker::require_not_banned(actor)?;
        if !actor.has_perm(ferum_domain::models::role::perm::REPORT_CREATE) {
            return Err(AppError::forbidden("permission_denied"));
        }
        if !actor.meets_trust(ferum_domain::models::user::TrustLevel::Basic) {
            return Err(AppError::forbidden("trust_level_insufficient"));
        }

        if cmd.post_id.is_none() && cmd.thread_id.is_none() {
            return Err(AppError::unprocessable("post_id or thread_id is required"));
        }

        if let Some(post_id) = cmd.post_id {
            self.posts.find_by_id(post_id).await?.or_not_found()?;
        }
        if let Some(thread_id) = cmd.thread_id {
            self.threads.find_by_id(thread_id).await?.or_not_found()?;
        }

        self.reports.create(actor.id, cmd.post_id, cmd.thread_id, cmd.reason).await
    }

    /// Reports the caller filed themselves — lets a reporter track resolution
    /// status without needing `moderation.view_reports`.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_my_reports(
        &self,
        actor: &AuthUser,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        self.reports.list_by_reporter(actor.id, page, per_page.min(50)).await
    }

    // ─── Report queue (moderator) ─────────────────────────────────────────────

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_pending_reports(
        &self,
        actor: &AuthUser,
        target_type: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        PermissionChecker::can_view_reports(actor, None)?;
        let cat_ids = actor.permitted_category_ids(ferum_domain::models::role::perm::MOD_VIEW_REPORTS);
        self.reports
            .list_all(
                Some(ReportStatus::Pending),
                target_type,
                None,
                cat_ids.as_deref(),
                page,
                per_page.min(50),
            )
            .await
    }

    pub async fn report_status_counts(
        &self,
        actor: &AuthUser,
    ) -> Result<ReportStatusCounts, AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.reports.count_by_status(None).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_all_reports(
        &self,
        actor: &AuthUser,
        status: Option<ReportStatus>,
        target_type: Option<&str>,
        q: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        self.reports.list_all(status, target_type, q, None, page, per_page.min(50)).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_all_reports_with_context(
        &self,
        actor: &AuthUser,
        status: Option<ReportStatus>,
        target_type: Option<&str>,
        q: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<ReportWithContext>, u64), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        let (reports, total) =
            self.reports.list_all(status, target_type, q, None, page, per_page.min(50)).await?;

        let reporter_ids: Vec<Uuid> = reports
            .iter()
            .map(|r| r.reporter_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let users = self.users.find_many_by_ids(&reporter_ids).await?;
        let user_map: HashMap<Uuid, String> =
            users.into_iter().map(|u| (u.id, u.username)).collect();

        let mut enriched = Vec::with_capacity(reports.len());
        for report in reports {
            let reporter_username = user_map
                .get(&report.reporter_id)
                .cloned()
                .unwrap_or_else(|| report.reporter_id.to_string());

            let (thread_slug, thread_title) = if let Some(thread_id) = report.thread_id {
                match self.threads.find_by_id(thread_id).await? {
                    Some(t) => (Some(t.slug), Some(t.title)),
                    None => (None, None),
                }
            } else if let Some(post_id) = report.post_id {
                match self.posts.find_by_id(post_id).await? {
                    Some(p) => match self.threads.find_by_id(p.thread_id).await? {
                        Some(t) => (Some(t.slug), Some(t.title)),
                        None => (None, None),
                    },
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            enriched.push(ReportWithContext { report, reporter_username, thread_slug, thread_title });
        }

        Ok((enriched, total))
    }

    /// Resolves the category a report's target (post or thread) belongs to.
    /// Returns `None` if the target was deleted/unreachable — callers must not
    /// fall back to a category-scoped permission check in that case, since there
    /// is no category left to verify scope against.
    async fn report_category_id(&self, report: &Report) -> Result<Option<Uuid>, AppError> {
        if let Some(thread_id) = report.thread_id {
            return Ok(self.threads.find_by_id(thread_id).await?.map(|t| t.category_id));
        }
        if let Some(post_id) = report.post_id {
            if let Some(post) = self.posts.find_by_id(post_id).await? {
                return Ok(self.threads.find_by_id(post.thread_id).await?.map(|t| t.category_id));
            }
        }
        Ok(None)
    }

    #[tracing::instrument(skip(self, actor, moderator_notes), fields(user_id = %actor.id, report_id = %report_id))]
    pub async fn resolve_report(
        &self,
        actor: &AuthUser,
        report_id: Uuid,
        status: ReportStatus,
        moderator_notes: Option<String>,
    ) -> Result<(), AppError> {
        // Upfront gate: actor must hold MOD_RESOLVE somewhere (global or any
        // category) before we even reveal whether report_id exists.
        PermissionChecker::can_resolve_report(actor, None)?;

        let report = self.reports.find_by_id(report_id).await?.or_not_found()?;

        // Now authorize against the report's ACTUAL category, not just "any
        // category this moderator happens to have MOD_RESOLVE in" — otherwise a
        // moderator scoped to category A could resolve/dismiss reports filed in
        // category B.
        match self.report_category_id(&report).await? {
            Some(category_id) => PermissionChecker::can_resolve_report(actor, Some(category_id))?,
            None => {
                // Target deleted/unreachable — no category to scope against, so
                // only a globally-permitted moderator/admin may act.
                if !actor.has_perm(ferum_domain::models::role::perm::MOD_RESOLVE) {
                    return Err(AppError::forbidden("permission_denied"));
                }
            }
        }

        if report.status != ferum_domain::models::report::ReportStatus::Pending {
            return Err(AppError::unprocessable("Report has already been resolved"));
        }
        self.reports.update_status(report_id, status.clone(), actor.id, moderator_notes).await?;

        let _ = self.cache.del("stats:dashboard").await;

        self.audit_log_repo
            .append(ferum_domain::models::audit_log::AuditLog::user_action(
                actor.id,
                &format!("report.{}", format!("{:?}", status).to_lowercase()),
                "report",
                report_id,
                None,
            ))
            .await
            .ok();

        Ok(())
    }

    // ─── User actions (moderator) ─────────────────────────────────────────────

    #[tracing::instrument(skip(self, actor, reason), fields(user_id = %actor.id, target_user_id = %user_id))]
    pub async fn warn_user(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
        reason: String,
    ) -> Result<(), AppError> {
        if reason.len() > MAX_BAN_REASON_LEN {
            return Err(AppError::unprocessable("Reason must be 1 000 characters or fewer"));
        }
        PermissionChecker::can_warn(actor)?;

        // Prevent warning users who also have warn permission (mods warning mods)
        // unless actor has admin-level manage_users permission
        // This is handled by the fact that admins have all permissions

        self.users.find_by_id(user_id).await?.or_not_found()?;

        self.users
            .update(user_id, UpdateUser { warn_count_delta: Some(1), ..Default::default() })
            .await?;

        self.notifications
            .create(
                user_id,
                ferum_domain::models::notification::NotificationKind::Warn,
                serde_json::json!({ "reason": reason, "actor_username": actor.username }),
            )
            .await?;

        self.event_bus
            .publish(ForumEvent::UserWarned { user_id, by_user_id: actor.id, reason })
            .await;

        // Each warning reduces trust_score. Fire-and-forget.
        {
            let users = self.users.clone();
            tokio::spawn(async move {
                let _ = users.increment_trust_score(user_id, -5).await;
            });
        }

        Ok(())
    }

    #[tracing::instrument(skip(self, actor, reason), fields(user_id = %actor.id, target_user_id = %user_id, until = %until))]
    pub async fn temp_ban(
        &self,
        actor: &AuthUser,
        user_id: Uuid,
        reason: String,
        until: DateTime<Utc>,
    ) -> Result<(), AppError> {
        if reason.len() > MAX_BAN_REASON_LEN {
            return Err(AppError::unprocessable("Reason must be 1 000 characters or fewer"));
        }
        PermissionChecker::can_ban_temp(actor)?;

        self.users.find_by_id(user_id).await?.or_not_found()?;

        let hook_ctx = HookContext {
            hook_name: "before_user_ban".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "target_user_id": user_id,
                "reason": reason,
                "until": until,
                "permanent": false,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_user_ban", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
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

        let ttl = (until - Utc::now()).to_std().unwrap_or(Duration::from_secs(0));
        if !ttl.is_zero() {
            self.cache
                .set(&format!("user:banned:{}", user_id), "1", ttl)
                .await
                .ok();
        }
        self.cache.del_prefix(&format!("refresh:{}:", user_id)).await.ok();

        // Invalidate user roles cache so next request re-resolves permissions
        self.cache.del(&format!("user:roles:{}", user_id)).await.ok();

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

    /// List reports with an optional status filter. Uses `can_view_reports` permission,
    /// so moderators (not just admins) can access this.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_reports(
        &self,
        actor: &AuthUser,
        status: Option<ReportStatus>,
        target_type: Option<&str>,
        q: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        PermissionChecker::can_view_reports(actor, None)?;
        let cat_ids = actor.permitted_category_ids(ferum_domain::models::role::perm::MOD_VIEW_REPORTS);
        self.reports
            .list_all(status, target_type, q, cat_ids.as_deref(), page, per_page.min(50))
            .await
    }

    /// Enriched version of `list_reports` — same `can_view_reports` permission so moderators
    /// can call it. Returns reporter username and thread context alongside each report.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_reports_with_context(
        &self,
        actor: &AuthUser,
        status: Option<ReportStatus>,
        target_type: Option<&str>,
        q: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<ReportWithContext>, u64), AppError> {
        PermissionChecker::can_view_reports(actor, None)?;
        let cat_ids = actor.permitted_category_ids(ferum_domain::models::role::perm::MOD_VIEW_REPORTS);
        let (reports, total) = self
            .reports
            .list_all(status, target_type, q, cat_ids.as_deref(), page, per_page.min(50))
            .await?;

        let reporter_ids: Vec<Uuid> = reports
            .iter()
            .map(|r| r.reporter_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let users = self.users.find_many_by_ids(&reporter_ids).await?;
        let user_map: HashMap<Uuid, String> =
            users.into_iter().map(|u| (u.id, u.username)).collect();

        let mut enriched = Vec::with_capacity(reports.len());
        for report in reports {
            let reporter_username = user_map
                .get(&report.reporter_id)
                .cloned()
                .unwrap_or_else(|| report.reporter_id.to_string());

            let (thread_slug, thread_title) = if let Some(thread_id) = report.thread_id {
                match self.threads.find_by_id(thread_id).await? {
                    Some(t) => (Some(t.slug), Some(t.title)),
                    None => (None, None),
                }
            } else if let Some(post_id) = report.post_id {
                match self.posts.find_by_id(post_id).await? {
                    Some(p) => match self.threads.find_by_id(p.thread_id).await? {
                        Some(t) => (Some(t.slug), Some(t.title)),
                        None => (None, None),
                    },
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            enriched.push(ReportWithContext { report, reporter_username, thread_slug, thread_title });
        }

        Ok((enriched, total))
    }

    pub async fn mod_report_status_counts(
        &self,
        actor: &AuthUser,
    ) -> Result<ReportStatusCounts, AppError> {
        PermissionChecker::can_view_reports(actor, None)?;
        let cat_ids = actor.permitted_category_ids(ferum_domain::models::role::perm::MOD_VIEW_REPORTS);
        self.reports.count_by_status(cat_ids.as_deref()).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_audit_log(
        &self,
        actor: &AuthUser,
        actor_id: Option<Uuid>,
        target_type: Option<&str>,
        action_contains: Option<&str>,
        created_from: Option<chrono::DateTime<chrono::Utc>>,
        created_to: Option<chrono::DateTime<chrono::Utc>>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<AuditLog>, u64), AppError> {
        PermissionChecker::can_view_reports(actor, None)?;

        // Audit entries record `target_type`/`target_id` (user, report, thread…),
        // and several actions — a permanent ban, a config change — belong to no
        // category at all, so there is no category to scope a moderator against.
        // Rather than leak every admin and peer-moderator action to anyone holding
        // `moderation.view_reports` in a single category, a non-admin only ever
        // sees entries they themselves authored. The caller-supplied `actor_id`
        // filter is overridden, not merely defaulted — otherwise passing another
        // user's id would walk straight around the restriction.
        let effective_actor_id = if actor.has_perm(ferum_domain::models::role::perm::ADMIN_USERS) {
            actor_id
        } else {
            Some(actor.id)
        };

        self.audit_log_repo
            .list(effective_actor_id, target_type, action_contains, created_from, created_to, page, per_page.min(50))
            .await
    }

    pub async fn search_users<'a>(
        &self,
        actor: &AuthUser,
        search: Option<&'a str>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<ferum_domain::models::User>, u64), AppError> {
        if !actor.has_perm(ferum_domain::models::role::perm::MOD_WARN)
            && !actor.has_perm(ferum_domain::models::role::perm::ADMIN_USERS)
        {
            return Err(AppError::forbidden("permission_denied"));
        }
        self.users.list_paginated(page, per_page, search, None, None).await
    }
}

#[derive(Debug)]
pub struct CreateReportCmd {
    pub post_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub reason: String,
}
