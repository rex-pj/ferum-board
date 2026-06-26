use axum::extract::{Extension, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

use super::super::require_moderator;

pub async fn users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));

    render_admin(&state, "mod/users.html", &ctx).await
}
