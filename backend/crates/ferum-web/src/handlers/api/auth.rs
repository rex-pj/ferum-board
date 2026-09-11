use axum::extract::{Extension, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::auth::{
    ForgotPasswordRequest, LoginRequest, LoginResponse, RegisterRequest,
    ResendVerificationRequest, ResetPasswordRequest, UserResponse,
};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::shared::AppError;
use ferum_application::usecases::auth_usecase::{
    LoginCmd, RefreshResult, RegisterCmd, ResetPasswordCmd,
};

pub async fn register(
    State(state): State<AppState>,
    Extension(locale): Extension<ferum_domain::Locale>,
    Json(body): Json<RegisterRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

    let user = state
        .auth
        .register(RegisterCmd {
            username: body.username,
            email: body.email,
            password: body.password,
            // The language they were reading the site in when they signed up —
            // the only signal available for a brand-new account with no stored
            // preference, and the verification email is the first thing they get.
            locale,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(UserResponse::from(user))),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

    let result = state
        .auth
        .login(LoginCmd {
            email: body.email,
            password: body.password,
        })
        .await?;

    let mut headers = HeaderMap::new();
    crate::utils::set_cookie(
        &mut headers,
        &crate::utils::refresh_token_cookie(&state, &result.refresh_token),
    );
    crate::utils::append_cookie(
        &mut headers,
        &crate::utils::access_token_cookie(&state, &result.access_token),
    );

    // Refresh the locale cookie from the account's stored preference, so signing
    // in on a new device brings the chosen language with it. A user who has never
    // chosen gets the cookie cleared rather than pinned, leaving them on
    // Accept-Language negotiation.
    match state.user.get_preferences_by_id(result.user.id).await {
        Ok(prefs) => {
            let cookie = match prefs.locale {
                Some(locale) => crate::utils::locale_cookie(&state, locale.as_str()),
                None => crate::utils::clear_locale_cookie(&state),
            };
            crate::utils::append_cookie(&mut headers, &cookie);
        }
        // A preferences read failure must not block a valid login; the user just
        // keeps whatever language the cookie already said.
        Err(e) => tracing::warn!(error = %e, "could not load locale preference at login"),
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(DataResponse::new(LoginResponse {
            user: UserResponse::from(result.user),
        })),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    headers: HeaderMap,
) -> HandlerResult<impl IntoResponse> {
    if let Some(user) = auth_user {
        // Revoke the access token unconditionally. Previously this whole block
        // was nested inside the refresh-cookie check, so a logout sent without
        // that cookie — expired, or a client holding only the access token —
        // cleared cookies browser-side and revoked nothing at all: the access
        // token stayed valid for the rest of its lifetime.
        match extract_refresh_cookie(&headers) {
            Some(token) => state.auth.logout(user.id, &token).await?,
            None => state.auth.logout_all(user.id).await?,
        }
    }

    let mut resp_headers = HeaderMap::new();
    crate::utils::set_cookie(
        &mut resp_headers,
        &crate::utils::clear_access_token_cookie(&state),
    );
    crate::utils::append_cookie(
        &mut resp_headers,
        &crate::utils::clear_refresh_token_cookie(&state),
    );

    Ok((StatusCode::NO_CONTENT, resp_headers))
}

pub async fn verify_email(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    state.auth.verify_email(&token).await?;
    Ok(Json(
        serde_json::json!({ "message": "Email verified successfully" }),
    ))
}

pub async fn resend_verification(
    State(state): State<AppState>,
    Json(body): Json<ResendVerificationRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    state.auth.resend_verification_email(&body.email).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "If that email exists and isn't verified yet, a new verification link has been sent."
        })),
    ))
}

pub async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    state.auth.forgot_password(&body.email).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "If that email exists, a reset link has been sent."
        })),
    ))
}

pub async fn reset_password(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<ResetPasswordRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    state
        .auth
        .reset_password(ResetPasswordCmd {
            token,
            new_password: body.new_password,
        })
        .await?;
    Ok(Json(
        serde_json::json!({ "message": "Password reset successfully" }),
    ))
}

pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> HandlerResult<impl IntoResponse> {
    let refresh_token = extract_refresh_cookie(&headers).ok_or(AppError::Unauthorized)?;
    let RefreshResult { access_token } = state.auth.refresh_access_token(&refresh_token).await?;

    let mut resp_headers = HeaderMap::new();
    crate::utils::set_cookie(
        &mut resp_headers,
        &crate::utils::access_token_cookie(&state, &access_token),
    );

    // Access token is delivered only via httpOnly cookie — never in the body.
    // Returning it in JSON would let XSS steal it from the refresh response.
    Ok((StatusCode::NO_CONTENT, resp_headers))
}

fn extract_refresh_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|part| {
                part.trim()
                    .strip_prefix("refresh_token=")
                    .map(|t| t.to_string())
            })
        })
}
