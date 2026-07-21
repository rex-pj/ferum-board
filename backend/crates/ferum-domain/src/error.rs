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
            AppError::Forbidden(c) => human_message(c),
            AppError::Conflict(c) => human_message(c),
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

/// Maps a machine error code (used by `Forbidden`/`Conflict`) to a sentence a
/// user can act on. Falls back to a space-separated rendering of the code for
/// anything not yet catalogued here, so new codes never crash — they just
/// read a bit raw until added below.
fn human_message(code: &str) -> String {
    let message = match code {
        "trust_level_insufficient" => {
            "Your account needs a higher trust level to do this. Keep participating to level up."
        }
        "category_closed" => "This category is closed to new posts.",
        "permission_denied" => "You don't have permission to do that.",
        "not_author" => "You can only do that with your own content.",
        "edit_window_expired" => "The 24-hour edit window for this has passed.",
        "thread_locked" => "This thread is locked.",
        "account_suspended" => "Your account has been suspended.",
        "account_locked" => "Your account is temporarily locked due to failed login attempts. Try again later.",
        "email_not_verified" => "Please verify your email address before continuing.",
        "no_password_set" => "This account has no password set yet.",
        "incorrect_current_password" => "Your current password is incorrect.",
        "tag_create_permission_required" => "You don't have permission to create new tags.",
        "invalid_or_expired_token" => "This link is invalid or has expired.",
        "token_already_used" => "This link has already been used.",
        "cannot_modify_system_role_permissions" => "Built-in role permissions can't be modified.",
        "cannot_grant_permissions_you_lack" => "You can't grant a permission you don't have yourself.",
        "cannot_remove_last_admin" => "You can't remove the last administrator.",
        "cannot_delete_system_role" => "Built-in roles can't be deleted.",
        "cannot_react_to_own_post" => "You can't react to your own post.",
        "registration_closed" => "Registration is currently closed.",
        "upload_quota_exceeded" => {
            "You've reached your daily upload limit. Try again tomorrow."
        }
        "media_capability_not_granted" => "This plugin isn't allowed to upload media.",
        "rpc_action_not_granted" => "This plugin action isn't available.",
        "sql_not_allowed" => "This database operation isn't allowed.",
        "role_already_assigned" => "That role is already assigned to this user.",
        "reaction_exists" => "You've already reacted with this.",
        "registration_conflict" => "An account with this email or username already exists.",
        "slug_taken" => "That name is already in use.",
        "category_has_subcategories" => "This category still has subcategories and can't be deleted.",
        "category_has_threads" => "This category still has threads and can't be deleted.",
        "email_taken" => "This email is already registered.",
        "username_taken" => "This username is already taken.",
        "plugin_already_installed" => "This plugin is already installed.",
        "product_already_reviewed" => {
            "You've already reviewed this product. Edit your existing review instead."
        }
        other => return other.replace('_', " "),
    };
    message.to_string()
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
