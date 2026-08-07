use std::time::Duration;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use validator::Validate;

use crate::app_state::AppState;
use crate::view_models::setup::{RunSetupRequest, SetupRunResponse, SetupStatusResponse};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::ports::RateLimitResult;
use ferum_application::shared::AppError;
use ferum_application::usecases::setup_usecase::{RunSetupCmd, SetupConfigCmd};

pub async fn status(
    State(state): State<AppState>,
) -> HandlerResult<impl IntoResponse> {
    let needs_setup = state.setup.needs_setup().await?;
    Ok(Json(DataResponse::new(SetupStatusResponse { needs_setup })))
}

pub async fn run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RunSetupRequest>,
) -> HandlerResult<impl IntoResponse> {
    // Rate limit: 5 attempts per 15 minutes per IP
    let ip = crate::middleware::rate_limit::extract_client_ip(&headers, state.trusted_proxy_count);
    let rl_key = format!("rl:setup:run:{}", ip);
    if let Ok(RateLimitResult::Denied { retry_after }) = state
        .rate_limiter
        .check(&rl_key, 5, Duration::from_secs(900))
        .await
    {
        return Err(AppError::TooManyRequests(retry_after.as_secs()).into());
    }
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

    let config = body.config.map(|c| SetupConfigCmd {
        site_name: c.site_name,
        site_tagline: c.site_tagline,
        primary_color: c.primary_color,
        registration_open: c.registration_open,
        smtp_host: c.smtp_host,
        smtp_port: c.smtp_port,
        smtp_user: c.smtp_user,
        smtp_pass: c.smtp_pass,
    });

    let result = state
        .setup
        .run_setup(RunSetupCmd {
            admin_username: body.admin_username,
            admin_email: body.admin_email,
            admin_password: body.admin_password,
            config,
            seed_example_data: body.seed_example_data,
        })
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        crate::utils::refresh_token_cookie(&state, &result.refresh_token)
            .parse()
            .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        crate::utils::access_token_cookie(&state, &result.access_token)
            .parse()
            .unwrap(),
    );

    Ok((
        StatusCode::CREATED,
        headers,
        Json(DataResponse::new(SetupRunResponse::new(
            result.user,
            result.access_token,
        ))),
    ))
}
