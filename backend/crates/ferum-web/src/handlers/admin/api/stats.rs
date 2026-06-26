use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::{DataResponse, HandlerResult};

#[derive(Deserialize)]
pub struct StatsHistoryQuery {
    pub metric: Option<String>,
    pub days: Option<u32>,
}

pub async fn stats(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let stats = state.admin_stats.dashboard(actor).await?;
    Ok(Json(DataResponse::new(stats)))
}

pub async fn stats_history(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<StatsHistoryQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let metric = q.metric.as_deref().unwrap_or("dau");
    let days = q.days.unwrap_or(30);
    let points = state.admin_stats.stats_history(actor, metric, days).await?;
    Ok(Json(DataResponse::new(points)))
}
