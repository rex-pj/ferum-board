//! `/admin/email-templates` — the editor for transactional email copy.
//!
//! Server-renders only the shell. The editor loads one template at a time and
//! previews without saving, which is several round trips that belong to
//! `ferum-admin-email-templates.js`.
//!
//! Gated on `admin.email_templates` specifically rather than the panel-wide
//! `require_admin`, following `/admin/languages`: the point of the separate
//! permission is that it can be granted on its own, and a page that admits any
//! `admin.*` holder would make that pointless.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use ferum_domain::models::role::perm;

use super::super::{render_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{require_page_auth, PageError};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

pub async fn email_templates(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    if !auth_user.has_perm(perm::ADMIN_EMAIL_TEMPLATES) {
        return Err(PageError::Unauthorized);
    }

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(
            &state,
            &auth_user,
            CurrentUserCtx::from(&auth_user),
        )
        .await,
    );
    // The editor needs the installed roster to offer a locale per template.
    ctx.insert(
        "template_locales",
        &state
            .translator
            .available_locales()
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>(),
    );

    render_admin(&state, &req_locale, "admin/email_templates.html", ctx).await
}
