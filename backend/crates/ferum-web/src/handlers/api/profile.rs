use axum::extract::{Extension, Multipart, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::auth::UserResponse;
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::constants::{JWT_EXPIRY_SECS, MAX_AVATAR_BYTES, MAX_COVER_BYTES};
use ferum_application::ports::AccessTokenClaims;
use ferum_application::shared::AppError;
use ferum_application::usecases::user_usecase::UpdateProfileCmd;
use ferum_domain::models::user::UserPreferences;

/// Re-mints the access token cookie after profile image changes so the header
/// avatar reflects the new URL immediately (without waiting for the JWT to expire).
fn refreshed_token_cookie(
    state: &AppState,
    actor: &AuthUser,
    avatar_url: Option<String>,
) -> Result<HeaderMap, AppError> {
    let claims = AccessTokenClaims {
        sub: actor.id,
        username: actor.username.clone(),
        display_name: actor.display_name.clone(),
        avatar_url,
        trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
        is_banned: actor.is_banned,
        banned_until: actor.banned_until.map(|t| t.timestamp()),
        exp: (chrono::Utc::now()
            + chrono::Duration::seconds(JWT_EXPIRY_SECS as i64))
        .timestamp(),
    };
    let token = state.token_service.mint_access_token(&claims)?;
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!("token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={JWT_EXPIRY_SECS}{secure}", token)
            .parse()
            .unwrap(),
    );
    Ok(headers)
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(max = 80))]
    pub display_name: Option<String>,
    #[validate(length(max = 500))]
    pub bio: Option<String>,
    #[validate(length(max = 200), url)]
    pub website: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePreferencesRequest {
    pub theme: Option<String>,
    pub font_size: Option<String>,
    pub layout: Option<String>,
    pub email_notifications: Option<serde_json::Value>,
    pub watched_categories: Option<Vec<uuid::Uuid>>,
    pub muted_categories: Option<Vec<uuid::Uuid>>,
}

pub async fn update_profile(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<UpdateProfileRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;

    let user = state
        .user
        .update_profile(
            actor,
            UpdateProfileCmd {
                display_name: body.display_name,
                bio: body.bio,
                website: body.website,
            },
        )
        .await?;

    Ok(Json(DataResponse::new(UserResponse::from(user))))
}

pub async fn change_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<ChangePasswordRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state
        .user
        .change_password(actor, &body.current_password, &body.new_password)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn get_preferences(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let prefs = state.user.get_preferences(actor).await?;
    Ok(Json(DataResponse::new(prefs)))
}

pub async fn update_preferences(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<UpdatePreferencesRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    // Load existing preferences so we can preserve fields the request doesn't set.
    let existing = state.user.get_preferences(actor).await?;

    let valid_themes = ["auto", "light", "dark"];
    let valid_font_sizes = ["small", "medium", "large"];
    let valid_layouts = ["compact", "comfortable"];

    let theme = body.theme.unwrap_or(existing.theme);
    let font_size = body.font_size.unwrap_or(existing.font_size);
    let layout = body.layout.unwrap_or(existing.layout);

    if !valid_themes.contains(&theme.as_str()) {
        return Err(AppError::unprocessable("theme must be auto, light, or dark").into());
    }
    if !valid_font_sizes.contains(&font_size.as_str()) {
        return Err(AppError::unprocessable("font_size must be small, medium, or large").into());
    }
    if !valid_layouts.contains(&layout.as_str()) {
        return Err(AppError::unprocessable("layout must be compact or comfortable").into());
    }

    let prefs = UserPreferences {
        user_id: actor.id,
        theme,
        font_size,
        layout,
        email_notifications: body
            .email_notifications
            .unwrap_or(existing.email_notifications),
        muted_categories: body.muted_categories.unwrap_or(existing.muted_categories),
        watched_categories: body.watched_categories.unwrap_or(existing.watched_categories),
    };

    state.user.update_preferences(actor, prefs).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /api/users/me/avatar — upload a new avatar image.
pub async fn upload_avatar(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut content_type = "application/octet-stream".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("file") {
            let ct = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();

            if !matches!(
                ct.as_str(),
                "image/jpeg" | "image/png" | "image/webp" | "image/gif"
            ) {
                return Err(AppError::UnprocessableEntity(
                    "Unsupported image type. Allowed: JPEG, PNG, WebP, GIF".to_string(),
                )
                .into());
            }

            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

            if data.len() > MAX_AVATAR_BYTES {
                return Err(
                    AppError::UnprocessableEntity(format!(
                        "File exceeds {} MB limit",
                        MAX_AVATAR_BYTES / (1024 * 1024)
                    ))
                    .into(),
                );
            }
            if !ferum_application::validators::validate_image_magic(&data) {
                return Err(AppError::UnprocessableEntity(
                    "File content does not match a supported image format (JPEG, PNG, WebP, GIF)"
                        .to_string(),
                )
                .into());
            }

            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;
    let url = state.user.set_avatar(actor, data, content_type).await?;
    let cookie_headers = refreshed_token_cookie(&state, actor, Some(url.clone()))?;
    Ok((
        cookie_headers,
        Json(serde_json::json!({ "data": { "avatar_url": url } })),
    ))
}

/// DELETE /api/users/me/avatar — remove the current avatar.
pub async fn delete_avatar(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.user.remove_avatar(actor).await?;
    let cookie_headers = refreshed_token_cookie(&state, actor, None)?;
    Ok((cookie_headers, StatusCode::NO_CONTENT))
}

/// POST /api/users/me/cover — upload a new cover image.
pub async fn upload_cover(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut content_type = "application/octet-stream".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("file") {
            let ct = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();

            if !matches!(
                ct.as_str(),
                "image/jpeg" | "image/png" | "image/webp" | "image/gif"
            ) {
                return Err(AppError::UnprocessableEntity(
                    "Unsupported image type. Allowed: JPEG, PNG, WebP, GIF".to_string(),
                )
                .into());
            }

            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

            if data.len() > MAX_COVER_BYTES {
                return Err(
                    AppError::UnprocessableEntity(format!(
                        "File exceeds {} MB limit",
                        MAX_COVER_BYTES / (1024 * 1024)
                    ))
                    .into(),
                );
            }
            if !ferum_application::validators::validate_image_magic(&data) {
                return Err(AppError::UnprocessableEntity(
                    "File content does not match a supported image format (JPEG, PNG, WebP, GIF)"
                        .to_string(),
                )
                .into());
            }

            content_type = ct;
            file_bytes = Some(data);
            break;
        }
    }

    let data = file_bytes
        .ok_or_else(|| AppError::UnprocessableEntity("Missing file field".to_string()))?;
    let url = state.user.set_cover(actor, data, content_type).await?;
    Ok(Json(serde_json::json!({ "data": { "cover_url": url } })))
}

/// DELETE /api/users/me/cover — remove the current cover image.
pub async fn delete_cover(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.user.remove_cover(actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

