use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::admin::api::config::{split_secrets, SMTP_PASS_KEY};
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

#[tracing::instrument(skip_all)]
pub async fn settings(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    // The cache is the whole `site_config` table, secrets included, and this page
    // renders it wholesale rather than through `CONFIG_READABLE_KEYS`. Strip the
    // secrets here and hand the template a presence flag instead, so no future
    // edit to settings.html can print one by accident.
    let (configs, secrets_set) = split_secrets(state.site_config_cache.read().await.clone());

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("config", &configs);
    ctx.insert(
        "smtp_pass_set",
        secrets_set.get(SMTP_PASS_KEY).unwrap_or(&false),
    );
    // Read from the live service rather than derived from the stored settings, so
    // the page reports the provider mail actually goes through. The two disagree
    // whenever RESEND_API_KEY is set: `config.smtp_host` is still populated and
    // still shown, but nothing sends through it.
    ctx.insert("mail_provider", state.email.provider().await.label());
    // Read-only: encryption at rest is an env-level decision, so the page reports
    // it rather than offering a control that could not take effect.
    ctx.insert("secrets_encrypted", &state.secrets_encrypted);

    render_admin(&state, &req_locale, "admin/settings.html", ctx).await
}
