use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{parse_date_from, parse_date_to, parse_opt_uuid, render_admin, reporting_tz_of, require_admin, site_ctx, PageQuery};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AdminReportCtx, AuditLogCtx, CurrentUserCtx, PaginationCtx};
use ferum_domain::models::report::ReportStatus;

#[tracing::instrument(skip_all, fields(page = q.page, status = q.status.as_deref()))]
pub async fn reports(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let page = crate::utils::page_number(q.page).map_err(|_| PageError::NotFound)?;
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
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("reports", &reports);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("filter", &filter);
    ctx.insert("search_query", &search);
    ctx.insert("count_pending",   &counts.pending);
    ctx.insert("count_resolved",  &counts.resolved);
    ctx.insert("count_dismissed", &counts.dismissed);

    render_admin(&state, &req_locale, "admin/moderation/reports.html", ctx).await
}

#[tracing::instrument(skip_all, fields(page = q.page, q = q.q.as_deref()))]
pub async fn audit_log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<PageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let page = crate::utils::page_number(q.page).map_err(|_| PageError::NotFound)?;
    let per_page = 50u64;
    let action_query = q.q.clone().unwrap_or_default();
    let actor_id_filter = q.actor_id.clone().unwrap_or_default();
    let target_type_filter = q.target_type.clone().unwrap_or_default();
    let date_from = q.date_from.clone().unwrap_or_default();
    let date_to = q.date_to.clone().unwrap_or_default();

    let actor_uuid = parse_opt_uuid(q.actor_id.as_deref());
    // The picked dates mean days in the site's reporting zone, so this filter
    // and the dashboard chart agree on where a day starts.
    let tz = reporting_tz_of(&state).await;

    let (logs, total) = state
        .admin
        .list_audit_log(
            &auth_user,
            actor_uuid,
            if target_type_filter.is_empty() { None } else { Some(target_type_filter.as_str()) },
            if action_query.is_empty() { None } else { Some(action_query.as_str()) },
            parse_date_from(q.date_from.as_deref(), tz),
            parse_date_to(q.date_to.as_deref(), tz),
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

    // ── Resolve actor and target labels in a fixed number of queries ─────────
    //
    // This used to be a `find_by_id` inside a loop over `logs`, per actor and
    // again per target — with the `"post"` arm doing two chained lookups of its
    // own. A page of 50 entries could issue over a hundred round-trips, under a
    // comment that said "Batch-fetch". It now costs four queries plus one for
    // posts, regardless of page size, using the `find_many_by_ids` methods the
    // repositories already expose.

    // Gather every id this page needs, deduplicated, before asking for anything.
    let mut actor_ids: Vec<uuid::Uuid> = Vec::new();
    let mut user_target_ids: Vec<uuid::Uuid> = Vec::new();
    let mut thread_target_ids: Vec<uuid::Uuid> = Vec::new();
    let mut post_target_ids: Vec<uuid::Uuid> = Vec::new();
    let mut category_target_ids: Vec<uuid::Uuid> = Vec::new();
    {
        let mut seen_actors = std::collections::HashSet::new();
        let mut seen_targets = std::collections::HashSet::new();
        for log in &logs {
            if let Some(aid) = log.actor_id {
                if seen_actors.insert(aid) {
                    actor_ids.push(aid);
                }
            }
            if !seen_targets.insert((log.target_type.as_str(), log.target_id)) {
                continue;
            }
            match log.target_type.as_str() {
                "user" => user_target_ids.push(log.target_id),
                "thread" => thread_target_ids.push(log.target_id),
                "post" => post_target_ids.push(log.target_id),
                "category" => category_target_ids.push(log.target_id),
                _ => {}
            }
        }
    }

    // An actor is a user, so both user lookups are one query. Categories come
    // from `list_all` — the set is small (two levels, bounded by what an admin
    // creates) and it is the same read the nav already performs, so a targeted
    // query would buy nothing.
    let (actors_and_targets, threads, posts, categories) = tokio::join!(
        async {
            let ids: Vec<uuid::Uuid> = actor_ids
                .iter()
                .chain(user_target_ids.iter())
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            if ids.is_empty() {
                return Vec::new();
            }
            state.user_repo.find_many_by_ids(&ids).await.unwrap_or_default()
        },
        async {
            if thread_target_ids.is_empty() {
                return Vec::new();
            }
            state
                .thread
                .threads
                .find_many_by_ids(&thread_target_ids)
                .await
                .unwrap_or_default()
        },
        async {
            if post_target_ids.is_empty() {
                return Vec::new();
            }
            state
                .moderation
                .posts
                .find_many_by_ids(&post_target_ids)
                .await
                .unwrap_or_default()
        },
        async {
            if category_target_ids.is_empty() {
                return Vec::new();
            }
            state.category.categories.list_all().await.unwrap_or_default()
        },
    );

    let users_by_id: std::collections::HashMap<uuid::Uuid, String> = actors_and_targets
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect();

    // A `"post"` entry is labelled with the thread that contains it, so the
    // posts resolved above name a second round of threads to fetch. One extra
    // query, not one per post.
    let mut target_thread_info: std::collections::HashMap<uuid::Uuid, (String, String)> =
        std::collections::HashMap::new(); // id → (title, slug)
    for t in threads {
        target_thread_info.insert(t.id, (t.title, t.slug));
    }
    if !posts.is_empty() {
        let parent_ids: Vec<uuid::Uuid> = posts
            .iter()
            .map(|p| p.thread_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let parents: std::collections::HashMap<uuid::Uuid, (String, String)> = state
            .thread
            .threads
            .find_many_by_ids(&parent_ids)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.id, (t.title, t.slug)))
            .collect();
        // Key by the POST's id: the render below looks the entry up by
        // `l.target_id`, which for a post entry is the post, not its thread.
        for p in &posts {
            if let Some(info) = parents.get(&p.thread_id) {
                target_thread_info.insert(p.id, info.clone());
            }
        }
    }

    let wanted_categories: std::collections::HashSet<uuid::Uuid> =
        category_target_ids.into_iter().collect();
    let target_category_info: std::collections::HashMap<uuid::Uuid, (String, String)> = categories
        .into_iter()
        .filter(|c| wanted_categories.contains(&c.id))
        .map(|c| (c.id, (c.name, c.slug)))
        .collect();

    // Actors and user targets share one map — both are usernames keyed by user
    // id, and splitting them only ever meant fetching the same row twice.
    let actor_names = &users_by_id;
    let target_user_names = &users_by_id;

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
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("entries", &entries);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("search_query", &action_query);
    ctx.insert("actor_id_filter", &actor_id_filter);
    ctx.insert("actor_label", &actor_label);
    ctx.insert("target_type_filter", &target_type_filter);
    ctx.insert("date_from_filter", &date_from);
    ctx.insert("date_to_filter", &date_to);

    render_admin(&state, &req_locale, "admin/moderation/audit_log.html", ctx).await
}
