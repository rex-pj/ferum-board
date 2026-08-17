//! `/admin/storage` — the surface for the two orphan-finding tools.
//!
//! Both already existed as JSON endpoints, which meant the real way to use them
//! was `curl` carrying an admin session cookie, for an operation that deletes
//! files irreversibly. This page exists so the list gets *read* before anything
//! acts on it.
//!
//! Deliberately server-renders nothing but the shell: scanning a bucket is paged
//! and can take several round trips, so the work belongs to
//! `ferum-admin-storage.js` where it can show progress. Rendering a first page
//! here would make the page load as slow as the scan.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{require_page_auth, PageError};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

pub async fn storage(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(
            &state,
            &auth_user,
            CurrentUserCtx::from(&auth_user),
        )
        .await,
    );

    render_admin(&state, &req_locale, "admin/storage.html", ctx).await
}
