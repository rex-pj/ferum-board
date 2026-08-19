use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, DashboardStatsCtx};

#[tracing::instrument(skip_all)]
pub async fn dashboard(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let stats = state.admin_stats.dashboard(&auth_user).await?;
    let stats_ctx = DashboardStatsCtx {
        total_users: stats.users_total,
        total_threads: stats.threads_total,
        total_posts: stats.posts_total,
        pending_reports: stats.reports_pending,
        new_users_today: stats.new_users_today,
        new_threads_today: stats.new_threads_today,
        new_posts_today: stats.new_posts_today,
        new_reactions_today: stats.new_reactions_today,
        new_views_today: stats.new_views_today,
        dau: stats.dau,
        mau: stats.mau,
        dau_mau_ratio: stats.dau_mau_ratio,
        activation_rate_pct: stats.activation_rate_pct,
        oldest_pending_report_hours: stats.oldest_pending_report_hours,
    };

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("stats", &stats_ctx);

    render_admin(&state, &req_locale, "admin/dashboard.html", ctx).await
}
