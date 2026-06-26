use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use tera::Context;
use tokio;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, PaginationCtx};
use ferum_domain::models::report::ReportStatus;

use super::super::{require_moderator, ModPageQuery};

pub async fn reports(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

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
        state.moderation.list_reports(&auth_user, status, None, q_str, page, per_page),
        state.moderation.mod_report_status_counts(&auth_user),
    )?;
    let (reports, total) = result;

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

    render_admin(&state, "mod/reports.html", &ctx).await
}
