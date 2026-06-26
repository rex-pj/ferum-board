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
    #[error("blocked by plugin: {error_code}")]
    PluginBlocked { reason: String, error_code: String },
}

impl AppError {
    pub fn forbidden(code: &str) -> Self {
        AppError::Forbidden(code.to_string())
    }

    pub fn unprocessable(message: &str) -> Self {
        AppError::UnprocessableEntity(message.to_string())
    }

    #[track_caller]
    pub fn internal(message: impl Into<String>) -> Self {
        let loc = std::panic::Location::caller();
        AppError::Internal(format!("{} [{}:{}]", message.into(), loc.file(), loc.line()))
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
            AppError::PluginBlocked { error_code, .. } => {
                (StatusCode::FORBIDDEN, error_code.as_str())
            }
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
        match status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                tracing::error!(
                    error.code = code,
                    error.detail = %self,
                    "internal_server_error"
                );
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                tracing::warn!(error.code = code, "auth_error");
            }
            StatusCode::TOO_MANY_REQUESTS => {
                tracing::warn!(error.code = code, "rate_limit_exceeded");
            }
            _ => {}
        }
        let message = match &self {
            AppError::Forbidden(c) => format!("Access denied: {}", c),
            AppError::Conflict(c) => format!("Conflict: {}", c),
            AppError::UnprocessableEntity(m) => m.clone(),
            AppError::TooManyRequests(retry) => {
                format!("Too many requests. Please try again in {} seconds.", retry)
            }
            AppError::Internal(_) => "An internal error occurred.".to_string(),
            AppError::PluginBlocked { reason, .. } => reason.clone(),
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

// ─── Option convenience ───────────────────────────────────────────────────────

pub trait OptionExt<T>: Sized {
    fn or_not_found(self) -> Result<T, AppError>;
}

impl<T> OptionExt<T> for Option<T> {
    fn or_not_found(self) -> Result<T, AppError> {
        self.ok_or(AppError::NotFound)
    }
}

impl From<sea_orm::DbErr> for AppError {
    #[track_caller]
    fn from(e: sea_orm::DbErr) -> Self {
        let loc = std::panic::Location::caller();
        let kind = match &e {
            sea_orm::DbErr::ConnectionAcquire(_) => "connection_acquire",
            sea_orm::DbErr::Exec(_) => "exec",
            sea_orm::DbErr::Query(_) => "query",
            sea_orm::DbErr::RecordNotFound(_) => "record_not_found",
            sea_orm::DbErr::RecordNotInserted => "record_not_inserted",
            sea_orm::DbErr::RecordNotUpdated => "record_not_updated",
            _ => "other",
        };
        tracing::error!(
            db.error_kind = kind,
            db.detail = %e,
            caller = %format!("{}:{}", loc.file(), loc.line()),
            "database_error"
        );
        AppError::Internal(format!("db:{kind} [{}:{}]", loc.file(), loc.line()))
    }
}
