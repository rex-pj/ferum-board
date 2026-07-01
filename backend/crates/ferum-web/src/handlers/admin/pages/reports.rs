use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{parse_date_from, parse_date_to, parse_opt_uuid, render_admin, require_admin, site_ctx, PageQuery};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AdminReportCtx, AuditLogCtx, CurrentUserCtx, PaginationCtx};
use ferum_domain::models::report::ReportStatus;

#[tracing::instrument(skip_all, fields(page = q.page, status = q.status.as_deref()))]
pub async fn reports(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;
    let filter = q.status.clone().unwrap_or_default();
    let search = q.q.clone().unwrap_or_default();

    let status = match filter.as_str() {
        "pending"   => Some(ReportStatus::Pending),
        "resolved"  => Some(ReportStatus::Resolved),
        "dismissed" => Some(ReportStatus::Dismissed),
        _           => None,
    };
    let q_str = if search.is_empty() { None } else { Some(search.as_str()) };

    let (result, counts) = tokio::try_join!(
        state.moderation.list_all_reports_with_context(&auth_user, status, None, q_str, page, per_page),
        state.moderation.report_status_counts(&auth_user),
    )?;
    let (raw_reports, total) = result;

    let reports: Vec<AdminReportCtx> = raw_reports
        .into_iter()
        .map(|r| AdminReportCtx {
            id: r.report.id.to_string(),
            reporter_id: r.report.reporter_id.to_string(),
            reporter_username: r.reporter_username,
            post_id: r.report.post_id.map(|id| id.to_string()),
            thread_id: r.report.thread_id.map(|id| id.to_string()),
            thread_slug: r.thread_slug,
            thread_title: r.thread_title,
            reason: r.report.reason,
            status: format!("{:?}", r.report.status).to_lowercase(),
            created_at: r.report.created_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("reports", &reports);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("filter", &filter);
    ctx.insert("search_query", &search);
    ctx.insert("count_pending",   &counts.pending);
    ctx.insert("count_resolved",  &counts.resolved);
    ctx.insert("count_dismissed", &counts.dismissed);

    render_admin(&state, "admin/moderation/reports.html", &ctx).await
}

#[tracing::instrument(skip_all, fields(page = q.page, q = q.q.as_deref()))]
pub async fn audit_log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 50u64;
    let action_query = q.q.clone().unwrap_or_default();
    let actor_id_filter = q.actor_id.clone().unwrap_or_default();
    let target_type_filter = q.target_type.clone().unwrap_or_default();
    let date_from = q.date_from.clone().unwrap_or_default();
    let date_to = q.date_to.clone().unwrap_or_default();

    let actor_uuid = parse_opt_uuid(q.actor_id.as_deref());

    let (logs, total) = state
        .admin
        .list_audit_log(
            &auth_user,
            actor_uuid,
            if target_type_filter.is_empty() { None } else { Some(target_type_filter.as_str()) },
            if action_query.is_empty() { None } else { Some(action_query.as_str()) },
            parse_date_from(q.date_from.as_deref()),
            parse_date_to(q.date_to.as_deref()),
            page,
            per_page,
        )
        .await?;

    let actor_label = match actor_uuid {
        Some(id) => state
            .admin
            .get_user(&auth_user, id)
            .await
            .ok()
            .map(|u| u.display_name.unwrap_or(u.username))
            .unwrap_or_default(),
        None => String::new(),
    };

    // Batch-fetch actor usernames for unique actor_ids in this page.
    let mut actor_names: std::collections::HashMap<uuid::Uuid, String> =
        std::collections::HashMap::new();
    for log in &logs {
        if let Some(aid) = log.actor_id {
            if !actor_names.contains_key(&aid) {
                if let Ok(Some(u)) = state.user_repo.find_by_id(aid).await {
                    actor_names.insert(aid, u.username);
                }
            }
        }
    }

    // Batch-fetch target labels and URLs by target_type.
    let mut target_user_names: std::collections::HashMap<uuid::Uuid, String> =
        std::collections::HashMap::new();
    let mut target_thread_info: std::collections::HashMap<uuid::Uuid, (String, String)> =
        std::collections::HashMap::new(); // id → (title, slug)
    let mut target_category_info: std::collections::HashMap<uuid::Uuid, (String, String)> =
        std::collections::HashMap::new(); // id → (name, slug)

    for log in &logs {
        match log.target_type.as_str() {
            "user" => {
                if !target_user_names.contains_key(&log.target_id) {
                    if let Ok(Some(u)) = state.user_repo.find_by_id(log.target_id).await {
                        target_user_names.insert(log.target_id, u.username);
                    }
                }
            }
            "thread" => {
                if !target_thread_info.contains_key(&log.target_id) {
                    if let Ok(Some(t)) = state.thread.threads.find_by_id(log.target_id).await {
                        target_thread_info.insert(log.target_id, (t.title, t.slug));
                    }
                }
            }
            "post" => {
                if !target_thread_info.contains_key(&log.target_id) {
                    if let Ok(Some(p)) = state.moderation.posts.find_by_id(log.target_id).await {
                        if let Ok(Some(t)) = state.thread.threads.find_by_id(p.thread_id).await {
                            target_thread_info.insert(log.target_id, (t.title, t.slug));
                        }
                    }
                }
            }
            "category" => {
                if !target_category_info.contains_key(&log.target_id) {
                    if let Ok(Some(c)) = state.category.categories.find_by_id(log.target_id).await {
                        target_category_info.insert(log.target_id, (c.name, c.slug));
                    }
                }
            }
            _ => {}
        }
    }

    let entries: Vec<AuditLogCtx> = logs
        .into_iter()
        .map(|l| {
            let aid_str = l.actor_id.map(|id| id.to_string()).unwrap_or_default();
            let uname = l.actor_id.and_then(|id| actor_names.get(&id)).cloned();
            let (target_label, target_url) = match l.target_type.as_str() {
                "user" => {
                    let label = target_user_names.get(&l.target_id).cloned();
                    let url = Some(format!("/admin/users/{}", l.target_id));
                    (label, url)
                }
                "thread" => {
                    if let Some((title, slug)) = target_thread_info.get(&l.target_id) {
                        (Some(title.clone()), Some(format!("/forum/t/{}", slug)))
                    } else {
                        (None, None)
                    }
                }
                "post" => {
                    if let Some((title, slug)) = target_thread_info.get(&l.target_id) {
                        (Some(title.clone()), Some(format!("/forum/t/{}", slug)))
                    } else {
                        (None, None)
                    }
                }
                "category" => {
                    if let Some((name, slug)) = target_category_info.get(&l.target_id) {
                        (Some(name.clone()), Some(format!("/forum/{}", slug)))
                    } else {
                        (None, None)
                    }
                }
                "report" => {
                    let short = &l.target_id.to_string()[..8];
                    (Some(format!("#{}", short)), None)
                }
                _ => (None, None),
            };
            AuditLogCtx {
                id: l.id.to_string(),
                actor_id: aid_str,
                actor_username: uname,
                action: l.action,
                target_type: l.target_type,
                target_id: l.target_id.to_string(),
                target_label,
                target_url,
                metadata: l.metadata,
                created_at: l.created_at.to_rfc3339(),
            }
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("entries", &entries);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("search_query", &action_query);
    ctx.insert("actor_id_filter", &actor_id_filter);
    ctx.insert("actor_label", &actor_label);
    ctx.insert("target_type_filter", &target_type_filter);
    ctx.insert("date_from_filter", &date_from);
    ctx.insert("date_to_filter", &date_to);

    render_admin(&state, "admin/moderation/audit_log.html", &ctx).await
}
