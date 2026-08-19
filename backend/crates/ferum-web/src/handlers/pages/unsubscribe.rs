//! `GET /unsubscribe/{token}` — one-click opt-out from notification email.
//!
//! No login wall and no confirmation step: the reader is usually signed out and
//! already annoyed, and the button that always works is "report as spam". The
//! signed token IS the authorisation, and it can only turn this user's mail off.
//!
//! Accepted cost: link prefetchers can unsubscribe someone who never clicked.
//! That is one preference restorable in `/account`; the opposite failure is paid
//! in domain reputation, which is not.

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
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
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
