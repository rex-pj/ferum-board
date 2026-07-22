//! Language switching for visitors who have no account to store a preference on.
//!
//! Signed-in users go through `PUT /api/users/me/preferences`, which persists the
//! choice and mirrors it into the same cookie this endpoint sets. This endpoint
//! exists because guests are 70% of the audience and must be able to read the
//! forum in their own language without registering.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{http::header, http::HeaderMap, http::StatusCode, Json};
use serde::Deserialize;

use ferum_domain::{AppError, AuthUser, Locale};

use crate::app_state::AppState;
use crate::view_models::HandlerResult;

#[derive(Debug, Deserialize)]
pub struct SetLocaleRequest {
    /// `null` clears the choice, returning the visitor to `Accept-Language`
    /// negotiation — the switcher's "Auto" option.
    pub locale: Option<Locale>,
}

/// `PUT /api/locale` — set the display language for this browser.
///
/// For a signed-in user this also persists to their account, so the choice
/// follows them to another device. That means the switcher in the nav can post
/// here unconditionally and not care whether anyone is logged in.
pub async fn set_locale(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Json(body): Json<SetLocaleRequest>,
) -> HandlerResult<impl IntoResponse> {
    if let Some(requested) = &body.locale {
        if !state.translator.available_locales().contains(requested) {
            return Err(AppError::invalid("locale_not_enabled").into());
        }
    }

    if let Some(actor) = &auth_user {
        // Best-effort: the cookie below is what actually changes the rendered
        // language, so a persistence failure should not make the switcher appear
        // broken. It is logged rather than surfaced.
        let mut prefs = state.user.get_preferences(actor).await?;
        prefs.locale = body.locale.clone();
        if let Err(e) = state.user.update_preferences(actor, prefs).await {
            tracing::warn!(error = %e, "could not persist locale preference");
        }
    }

    let cookie = match &body.locale {
        Some(locale) => crate::utils::locale_cookie(&state, locale.as_str()),
        None => crate::utils::clear_locale_cookie(&state),
    };
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());

    Ok((StatusCode::NO_CONTENT, headers))
}
