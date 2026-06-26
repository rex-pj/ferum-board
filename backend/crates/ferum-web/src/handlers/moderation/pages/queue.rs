use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::{render_admin, site_ctx};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, PaginationCtx};

use super::super::{require_moderator, ModPageQuery};

pub async fn queue(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ModPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_moderator(&auth_user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = 20u64;

    let (posts, total): (Vec<String>, u64) = (vec![], 0);

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("posts", &posts);
    ctx.insert("pagination", &PaginationCtx::simple(page, per_page, total));

    render_admin(&state, "mod/queue.html", &ctx).await
}
