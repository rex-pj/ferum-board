use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use std::collections::HashMap;
use tera::Context;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::admin::{parse_date_from, parse_date_to, parse_opt_uuid, render_admin, reporting_tz_of, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{AuditLogCtx, CurrentUserCtx, PaginationCtx};

use super::super::{require_moderator, ModPageQuery};

pub async fn log(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = crate::utils::page_number(q.page).map_err(|_| PageError::NotFound)?;
    let per_page = 30u64;

    // `list_audit_log` pins a non-admin's view to their own entries, so an actor
    // filter is meaningless for them — every row already has them as the actor.
    // Drop the parameter here too, or the page would render a filter chip and
    // paginate with an `actor_id` the use case is going to ignore anyway.
    let can_filter_by_actor = auth_user.has_perm(ferum_domain::models::role::perm::ADMIN_USERS);

    let action_query  = q.q.clone().unwrap_or_default();
    let actor_filter  = if can_filter_by_actor { q.actor_id.clone().unwrap_or_default() } else { String::new() };
    let target_filter = q.target_type.clone().unwrap_or_default();
    let date_from_str = q.date_from.clone().unwrap_or_default();
    let date_to_str   = q.date_to.clone().unwrap_or_default();

    let actor_uuid    = if can_filter_by_actor { parse_opt_uuid(q.actor_id.as_deref()) } else { None };
    let target_param  = if target_filter.is_empty() { None } else { Some(target_filter.as_str()) };
    let action_param  = if action_query.is_empty()  { None } else { Some(action_query.as_str()) };

    // Picked dates mean days in the site's reporting zone, not UTC.
    let tz = reporting_tz_of(&state).await;

    let (logs, total) = state
        .moderation
        .list_audit_log(
            &auth_user,
            actor_uuid,
            target_param,
            action_param,
            parse_date_from(q.date_from.as_deref(), tz),
            parse_date_to(q.date_to.as_deref(), tz),
            page,
            per_page,
        )
        .await?;

    // Batch-fetch actor usernames.
    let mut actor_names: HashMap<Uuid, String> = HashMap::new();
    for log in &logs {
        if let Some(aid) = log.actor_id {
            if let std::collections::hash_map::Entry::Vacant(e) = actor_names.entry(aid) {
                if let Ok(Some(u)) = state.user_repo.find_by_id(aid).await {
                    e.insert(u.username);
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
                if let std::collections::hash_map::Entry::Vacant(e) = target_user_names.entry(log.target_id) {
                    if let Ok(Some(u)) = state.user_repo.find_by_id(log.target_id).await {
                        e.insert(u.username);
                    }
                }
            }
            "thread" => {
                if let std::collections::hash_map::Entry::Vacant(e) = target_thread_info.entry(log.target_id) {
                    if let Ok(Some(t)) = state.thread.threads.find_by_id(log.target_id).await {
                        e.insert((t.title, t.slug));
                    }
                }
            }
            "post" => {
                if let std::collections::hash_map::Entry::Vacant(e) = target_thread_info.entry(log.target_id) {
                    if let Ok(Some(p)) = state.moderation.posts.find_by_id(log.target_id).await {
                        if let Ok(Some(t)) = state.thread.threads.find_by_id(p.thread_id).await {
                            e.insert((t.title, t.slug));
                        }
                    }
                }
            }
            "category" => {
                if let std::collections::hash_map::Entry::Vacant(e) = target_category_info.entry(log.target_id) {
                    if let Ok(Some(c)) = state.category.categories.find_by_id(log.target_id).await {
                        e.insert((c.name, c.slug));
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
                    let short = crate::utils::short_id(&l.target_id);
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
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("entries", &entries);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));
    ctx.insert("search_query", &action_query);
    ctx.insert("can_filter_by_actor", &can_filter_by_actor);
    ctx.insert("actor_id_filter", &actor_filter);
    ctx.insert("actor_label", &actor_label);
    ctx.insert("target_type_filter", &target_filter);
    ctx.insert("date_from_filter", &date_from_str);
    ctx.insert("date_to_filter", &date_to_str);

    render_admin(&state, &req_locale, "mod/log.html", ctx).await
}
