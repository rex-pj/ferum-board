use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

#[tracing::instrument(skip_all)]
pub async fn settings(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let configs = state.site_config_cache.read().await.clone();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("config", &configs);

    render_admin(&state, "admin/settings.html", &ctx).await
}
