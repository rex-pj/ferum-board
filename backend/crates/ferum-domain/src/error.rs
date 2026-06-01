use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),
    #[error("too many requests")]
    TooManyRequests(u64),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn forbidden(code: &str) -> Self {
        AppError::Forbidden(code.to_string())
    }

    pub fn unprocessable(message: &str) -> Self {
        AppError::UnprocessableEntity(message.to_string())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal(message.into())
    }

    pub fn status_and_code(&self) -> (StatusCode, &str) {
        match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden(code) => (StatusCode::FORBIDDEN, code.as_str()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Conflict(code) => (StatusCode::CONFLICT, code.as_str()),
            AppError::UnprocessableEntity(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "validation_error")
            }
            AppError::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let retry_after = if let AppError::TooManyRequests(secs) = &self {
            Some(*secs)
        } else {
            None
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "internal server error");
        }
        let message = match &self {
            AppError::Forbidden(c) => format!("Access denied: {}", c),
            AppError::Conflict(c) => format!("Conflict: {}", c),
            AppError::UnprocessableEntity(m) => m.clone(),
            AppError::TooManyRequests(retry) => {
                format!("Too many requests. Please try again in {} seconds.", retry)
            }
            AppError::Internal(_) => "An internal error occurred.".to_string(),
            _ => code.replace('_', " "),
        };
        let body = json!({ "error": { "code": code, "message": message } });
        let mut res = (status, axum::Json(body)).into_response();
        if let Some(secs) = retry_after {
            res.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_str(&secs.to_string()).unwrap(),
            );
        }
        res
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(e: sea_orm::DbErr) -> Self {
        tracing::error!("database error: {:?}", e);
        AppError::Internal(e.to_string())
    }
}
