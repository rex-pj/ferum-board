use axum::extract::{Extension, Form, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::handlers::admin::site_ctx;
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

use super::{active_theme, render_with_theme, PageError};

#[derive(Deserialize, Default)]
pub struct LoginPageQuery {
    pub error: Option<String>,
    pub verified: Option<String>,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginFormBody {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<LoginPageQuery>,
) -> Result<impl IntoResponse, PageError> {
    if auth_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }

    let active = active_theme(&state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &Option::<CurrentUserCtx>::None);
    ctx.insert("active_theme", &active);
    ctx.insert("error", &q.error);
    ctx.insert("verified", &q.verified.is_some());

    render_with_theme(&state, &active, "auth/login.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn register(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    if auth_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }

    let configs = state.site_config.get_all().await.unwrap_or_default();
    let registration_open = configs
        .get("registration_open")
        .map(|v| v == "true")
        .unwrap_or(true);

    let active = active_theme(&state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &Option::<CurrentUserCtx>::None);
    ctx.insert("active_theme", &active);
    ctx.insert("registration_open", &registration_open);

    render_with_theme(&state, &active, "auth/register.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn forgot_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> Result<impl IntoResponse, PageError> {
    if auth_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }

    let active = active_theme(&state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &Option::<CurrentUserCtx>::None);
    ctx.insert("active_theme", &active);

    render_with_theme(&state, &active, "auth/forgot_password.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

pub async fn reset_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<TokenQuery>,
) -> Result<impl IntoResponse, PageError> {
    if auth_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }

    let active = active_theme(&state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &Option::<CurrentUserCtx>::None);
    ctx.insert("active_theme", &active);
    ctx.insert("token", &q.token.unwrap_or_default());

    render_with_theme(&state, &active, "auth/reset_password.html", &ctx)
        .await
        .map(IntoResponse::into_response)
}

/// GET /verify-email/{token} — consumes the emailed link and redirects to
/// /login with a success or error flag (the API endpoint of the same name
/// returns bare JSON and is not meant to be opened directly in a browser).
pub async fn verify_email(
    State(state): State<AppState>,
    axum::extract::Path(token): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.auth.verify_email(&token).await {
        Ok(_) => Redirect::to("/login?verified=1"),
        Err(_) => Redirect::to("/login?error=verify_failed"),
    }
}

// ─── Form POST fallbacks (for password managers / no-JS) ─────────────────────

pub async fn login_post(
    State(state): State<AppState>,
    Form(body): Form<LoginFormBody>,
) -> impl IntoResponse {
    use ferum_application::usecases::auth_usecase::LoginCmd;

    let result = state
        .auth
        .login(LoginCmd {
            email: body.email,
            password: body.password,
        })
        .await;

    match result {
        Ok(r) => {
            let refresh_cookie = crate::utils::refresh_token_cookie(&state, &r.refresh_token);
            let token_cookie = crate::utils::access_token_cookie(&state, &r.access_token);
            (
                [
                    (header::SET_COOKIE, refresh_cookie),
                    (header::SET_COOKIE, token_cookie),
                ],
                Redirect::to("/"),
            )
                .into_response()
        }
        Err(e) => {
            // Distinguish "wrong password" from cases where the password was
            // actually correct but something else blocks login — the no-JS
            // fallback previously collapsed all of these into the same
            // "Invalid email or password" message, which is simply false for
            // a locked/unverified/banned account.
            let code = match &e {
                ferum_application::shared::AppError::Forbidden(c) => c.as_str(),
                _ => "invalid",
            };
            Redirect::to(&format!("/login?error={code}")).into_response()
        }
    }
}

pub async fn register_post() -> impl IntoResponse {
    Redirect::to("/register")
}

pub async fn forgot_password_post() -> impl IntoResponse {
    Redirect::to("/forgot-password")
}

pub async fn reset_password_post() -> impl IntoResponse {
    Redirect::to("/reset-password")
}
