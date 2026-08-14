//! `GET /unsubscribe/{token}` — one-click opt-out from notification email.
//!
//! ## Why this is a GET with no confirmation step
//!
//! Because the alternative is worse. Someone clicking unsubscribe in a mail client
//! is usually not logged in and is already mildly annoyed; a login wall, or a
//! "are you sure?" form, is one more obstacle between them and the outcome they
//! asked for — and the button that always works is "report as spam". A signed,
//! long-lived token in the URL is the authorisation, and the action it permits is
//! narrow enough to be safe without a session: it can only ever turn this one
//! user's mail off, never read anything, never grant access.
//!
//! Mail clients and security scanners do prefetch links, so a prefetch can
//! unsubscribe someone who never clicked. That is a real cost, accepted
//! deliberately: the outcome is one preference the user can restore in `/account`
//! with two clicks, and the page says so. The opposite trade — a link that
//! sometimes fails to work — is paid in domain reputation, which is not
//! recoverable.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

use super::{active_theme, render_with_theme_in, user_ctx, PageError};

pub async fn unsubscribe(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, PageError> {
    // An invalid or expired token is rendered as a page, not returned as an error
    // status. The reader is a person who clicked a link in an email, and a bare 403
    // tells them nothing about what to do next; the template offers `/account`
    // instead.
    let ok = state.user.unsubscribe_from_emails(&token).await.is_ok();
    if !ok {
        tracing::info!("unsubscribe link rejected (invalid or expired token)");
    }

    let active = active_theme(&state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    // Renders for a signed-in reader and a stranger alike — the token is what
    // authorises the change, so the nav simply reflects whoever is looking.
    match &auth_user {
        Some(_) => ctx.insert("current_user", &user_ctx(&state, auth_user.as_ref()).await),
        None => ctx.insert("current_user", &Option::<CurrentUserCtx>::None),
    }
    ctx.insert("active_theme", &active);
    ctx.insert("success", &ok);

    render_with_theme_in(&state, &req_locale, &active, "app/unsubscribed.html", ctx)
        .await
        .map(IntoResponse::into_response)
}
