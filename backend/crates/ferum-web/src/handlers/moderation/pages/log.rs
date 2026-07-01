use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use std::collections::HashMap;
use tera::Context;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::admin::{parse_date_from, parse_date_to, parse_opt_uuid, render_admin, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AuditLogCtx, CurrentUserCtx, PaginationCtx};

use super::super::{require_moderator, ModPageQuery};

pub async fn log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 30u64;

    let action_query  = q.q.clone().unwrap_or_default();
    let actor_filter  = q.actor_id.clone().unwrap_or_default();
    let target_filter = q.target_type.clone().unwrap_or_default();
    let date_from_str = q.date_from.clone().unwrap_or_default();
    let date_to_str   = q.date_to.clone().unwrap_or_default();

    let actor_uuid    = parse_opt_uuid(q.actor_id.as_deref());
    let target_param  = if target_filter.is_empty() { None } else { Some(target_filter.as_str()) };
    let action_param  = if action_query.is_empty()  { None } else { Some(action_query.as_str()) };

    let (logs, total) = state
        .moderation
        .list_audit_log(
            &auth_user,
            actor_uuid,
            target_param,
            action_param,
            parse_date_from(q.date_from.as_deref()),
            parse_date_to(q.date_to.as_deref()),
            page,
            per_page,
        )
        .await?;

    // Batch-fetch actor usernames.
    let mut actor_names: HashMap<Uuid, String> = HashMap::new();
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
    let mut target_user_names: HashMap<Uuid, String> = HashMap::new();
    let mut target_thread_info: HashMap<Uuid, (String, String)> = HashMap::new(); // (title, slug)
    let mut target_category_info: HashMap<Uuid, (String, String)> = HashMap::new(); // (name, slug)

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

    // Resolve actor label for the filter chip.
    let actor_label = match actor_uuid {
        Some(id) => actor_names.get(&id).cloned().unwrap_or_default(),
        None => String::new(),
    };

    let entries: Vec<AuditLogCtx> = logs
        .into_iter()
        .map(|l| {
            let actor_username = l.actor_id.and_then(|id| actor_names.get(&id)).cloned();
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
                actor_id: l.actor_id.map(|id| id.to_string()).unwrap_or_default(),
                actor_username,
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
    ctx.insert("actor_id_filter", &actor_filter);
    ctx.insert("actor_label", &actor_label);
    ctx.insert("target_type_filter", &target_filter);
    ctx.insert("date_from_filter", &date_from_str);
    ctx.insert("date_to_filter", &date_to_str);

    render_admin(&state, "mod/log.html", &ctx).await
}
