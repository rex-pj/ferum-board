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

    let secure = if state.cookies_secure { "; Secure" } else { "" };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!(
            "refresh_token={}; HttpOnly; SameSite=Lax; Path=/api/auth; Max-Age=604800{}",
            result.refresh_token, secure
        )
        .parse()
        .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        format!(
            "token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600{}",
            result.access_token, secure
        )
        .parse()
        .unwrap(),
    );

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
        if let Some(token) = extract_refresh_cookie(&headers) {
            state.auth.logout(user.id, &token).await?;
        }
    }

    let secure = if state.cookies_secure { "; Secure" } else { "" };
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::SET_COOKIE,
        format!(
            "token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{}",
            secure
        )
        .parse()
        .unwrap(),
    );
    resp_headers.append(
        header::SET_COOKIE,
        format!(
            "refresh_token=; HttpOnly; SameSite=Lax; Path=/api/auth; Max-Age=0{}",
            secure
        )
        .parse()
        .unwrap(),
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

    let secure = if state.cookies_secure { "; Secure" } else { "" };
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::SET_COOKIE,
        format!(
            "token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600{}",
            access_token, secure
        )
        .parse()
        .unwrap(),
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
