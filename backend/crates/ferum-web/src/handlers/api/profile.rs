use axum::extract::{Extension, Multipart, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::utils::{validate_upload_image, ImageKind};
use crate::view_models::auth::UserResponse;
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::constants::{MAX_AVATAR_BYTES, MAX_COVER_BYTES};
use ferum_application::ports::AccessTokenClaims;
use ferum_application::shared::AppError;
use ferum_application::usecases::user_usecase::UpdateProfileCmd;
use ferum_domain::models::user::UserPreferences;

/// Re-mints the access token cookie after profile image changes so the header
/// avatar reflects the new URL immediately (without waiting for the JWT to expire).
/// `issued_at` overrides the token's `iat`. Callers that have just published a
/// session epoch must pass it, or the replacement token is itself older than
/// the epoch and gets revoked on the very next request.
fn refreshed_token_cookie_at(
    state: &AppState,
    actor: &AuthUser,
    avatar_url: Option<String>,
    issued_at: Option<i64>,
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
            + chrono::Duration::seconds(state.token_service.access_token_ttl_secs() as i64))
        .timestamp(),
        iat: issued_at.unwrap_or_else(|| chrono::Utc::now().timestamp()),
    };
    let token = state.token_service.mint_access_token(&claims)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        crate::utils::access_token_cookie(state, &token).parse().unwrap(),
    );
    Ok(headers)
}

/// Re-mint with the current time — for callers that have not revoked anything.
fn refreshed_token_cookie(
    state: &AppState,
    actor: &AuthUser,
    avatar_url: Option<String>,
) -> Result<HeaderMap, AppError> {
    refreshed_token_cookie_at(state, actor, avatar_url, None)
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
    /// Display language. Absent leaves the current choice alone; an explicit
    /// `null` clears it, returning the user to site-default negotiation. That
    /// three-way distinction is why this is a nested `Option`.
    #[serde(default, deserialize_with = "deserialize_optional_locale")]
    pub locale: Option<Option<ferum_domain::Locale>>,
    /// Display timezone, as an IANA name. Same three-way shape as `locale`:
    /// absent leaves the current choice alone, an explicit `null` clears it and
    /// returns the user to whatever zone their device reports.
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub timezone: Option<Option<String>>,
}

/// Distinguishes "field absent" from "field explicitly null".
///
/// `Option<Option<T>>` with `#[serde(default)]` alone cannot do this — serde
/// collapses both to `None`. Going through `deserialize_with` makes an explicit
/// `"locale": null` arrive as `Some(None)`, which is the "reset me to the site
/// default" signal the switcher's *Auto* option needs.
fn deserialize_optional_locale<'de, D>(
    deserializer: D,
) -> Result<Option<Option<ferum_domain::Locale>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<ferum_domain::Locale>::deserialize(deserializer).map(Some)
}

/// Same absent-vs-explicit-null trick as `deserialize_optional_locale`, for a
/// plain string field.
fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
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
    let epoch = state
        .user
        .change_password(actor, &body.current_password, &body.new_password)
        .await?;

    // Changing the password revokes every token issued before the epoch —
    // including the one this very request authenticated with. Hand back a
    // freshly minted one so the person who just rotated their own password
    // stays signed in, while every *other* session (the reason to rotate it)
    // ends. It must carry the epoch as its `iat`: minted with the current
    // second instead, it would sort *before* the epoch and be rejected on the
    // very next request.
    let headers = refreshed_token_cookie_at(&state, actor, actor.avatar_url.clone(), epoch)?;
    Ok((axum::http::StatusCode::NO_CONTENT, headers))
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
        return Err(AppError::invalid("invalid_theme").into());
    }
    if !valid_font_sizes.contains(&font_size.as_str()) {
        return Err(AppError::invalid("invalid_font_size").into());
    }
    if !valid_layouts.contains(&layout.as_str()) {
        return Err(AppError::invalid("invalid_layout").into());
    }

    // Unlike theme/font_size/layout, the valid set here is not a fixed array —
    // it is whatever the site currently has installed. Validating against the
    // live roster means removing a language pack immediately stops anyone from
    // selecting it, with no code change and no stale allowlist.
    let locale = match body.locale {
        None => existing.locale,
        Some(None) => None,
        Some(Some(requested)) => {
            if !state.translator.available_locales().contains(&requested) {
                return Err(AppError::invalid("locale_not_enabled").into());
            }
            Some(requested)
        }
    };

    // Same shape as `locale` above: absent = leave alone, explicit null = clear
    // back to "follow the device". The valid set is again not a fixed array —
    // it is whatever the runtime's timezone database knows, so a zone added by
    // a future tzdata update works with no code change.
    let timezone = match body.timezone {
        None => existing.timezone,
        Some(None) => None,
        Some(Some(requested)) => {
            let trimmed = requested.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.parse::<chrono_tz::Tz>().is_err() {
                return Err(AppError::invalid_with(
                    "invalid_timezone",
                    [("tz", ferum_domain::i18n::TransArg::Str(trimmed.to_string()))],
                )
                .into());
            } else {
                Some(trimmed.to_string())
            }
        }
    };

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
        locale: locale.clone(),
        timezone,
    };

    state.user.update_preferences(actor, prefs).await?;

    // Mirror the durable choice into the cookie the negotiator reads, so the very
    // next page render is already in the new language — without this the user
    // would save a language and see no change until their cookie happened to be
    // refreshed elsewhere.
    let cookie = match locale {
        Some(l) => crate::utils::locale_cookie(&state, l.as_str()),
        None => crate::utils::clear_locale_cookie(&state),
    };
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());

    Ok((axum::http::StatusCode::NO_CONTENT, headers))
}

/// POST /api/users/me/avatar — upload a new avatar image.
pub async fn upload_avatar(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;

    let (data, content_type, crop) =
        crate::utils::read_image_field_with_crop(&mut multipart, "file").await?;
    validate_upload_image(&content_type, &data, MAX_AVATAR_BYTES, ImageKind::AVATAR)?;

    let url = state.user.set_avatar(actor, data, content_type, crop).await?;
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

    let (data, content_type, crop) =
        crate::utils::read_image_field_with_crop(&mut multipart, "file").await?;
    validate_upload_image(&content_type, &data, MAX_COVER_BYTES, ImageKind::COVER)?;

    let url = state.user.set_cover(actor, data, content_type, crop).await?;
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

