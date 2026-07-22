use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

use crate::i18n::{error_key, TransArg};

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
    /// Validation failure carrying **free-form English prose**.
    ///
    /// Retained only for genuinely dynamic text that has no stable code — in
    /// practice `validator::ValidationErrors::to_string()`, whose content is
    /// assembled per-field at runtime. This variant is **not translatable**;
    /// translating it requires mapping validator's per-field codes, which is
    /// tracked separately in docs/i18n-plan.md.
    ///
    /// For anything with a fixed meaning, use [`AppError::Invalid`] instead so
    /// the message lives in the catalog and clients can branch on the code.
    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),
    /// Validation failure identified by a stable machine code.
    ///
    /// Unlike `UnprocessableEntity`, the code is part of the API contract and
    /// the human text lives in the translation catalog, so a client can branch
    /// on `code` and a user can read it in their own language.
    #[error("invalid: {code}")]
    Invalid {
        code: String,
        args: Vec<(String, TransArg)>,
    },
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

    /// A validation failure with a stable code and no interpolated values.
    pub fn invalid(code: &str) -> Self {
        AppError::Invalid {
            code: code.to_string(),
            args: Vec::new(),
        }
    }

    /// A validation failure whose message interpolates runtime values.
    ///
    /// Prefer this over baking a number into the message text: the limit then
    /// lives in exactly one place (the constant), and translators can move it
    /// wherever their language's word order requires.
    ///
    /// ```ignore
    /// AppError::invalid_with("post_content_too_long", [("limit_kb", (MAX_POST_BYTES / 1024).into())])
    /// ```
    pub fn invalid_with<I, K>(code: &str, args: I) -> Self
    where
        I: IntoIterator<Item = (K, TransArg)>,
        K: Into<String>,
    {
        AppError::Invalid {
            code: code.to_string(),
            args: args.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
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
            AppError::Invalid { code, .. } => (StatusCode::UNPROCESSABLE_ENTITY, code.as_str()),
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
        // Which errors carry text the catalog owns, and with what arguments.
        //
        // `PluginBlocked` is deliberately excluded: its `reason` is authored by
        // a third-party plugin at runtime, so there is no key for it and no
        // catalog we could translate it against.
        let payload = match &self {
            AppError::Forbidden(c) | AppError::Conflict(c) => {
                Some(ErrorPayload::new(c.clone(), Vec::new()))
            }
            AppError::Invalid { code, args } => {
                Some(ErrorPayload::new(code.clone(), args.clone()))
            }
            AppError::TooManyRequests(retry) => Some(ErrorPayload::new(
                "rate_limit_exceeded".to_string(),
                vec![("seconds".to_string(), TransArg::Int(*retry as i64))],
            )),
            AppError::Unauthorized => Some(ErrorPayload::new("unauthorized".to_string(), Vec::new())),
            AppError::NotFound => Some(ErrorPayload::new("not_found".to_string(), Vec::new())),
            AppError::Internal(_) => {
                Some(ErrorPayload::new("internal_error".to_string(), Vec::new()))
            }
            // Free-form prose and plugin-authored text pass through untouched.
            AppError::UnprocessableEntity(_) | AppError::PluginBlocked { .. } => None,
        };

        // The body is written with an *untranslated* placeholder. The
        // `translate_errors` middleware rewrites `message` using the request's
        // locale before the response leaves the server.
        //
        // Doing it in middleware rather than here is what keeps this crate free
        // of a global translator handle: `IntoResponse` has no access to
        // `AppState` or to request extensions, so the only alternatives would be
        // a process-wide static or a task-local — both of which this codebase
        // deliberately avoids.
        let message = match &self {
            AppError::UnprocessableEntity(m) => m.clone(),
            AppError::PluginBlocked { reason, .. } => reason.clone(),
            // Readable degradation if the middleware is ever absent: the user
            // sees "thread locked" rather than a blank string.
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
        if let Some(payload) = payload {
            res.extensions_mut().insert(payload);
        }
        res
    }
}

/// Carries an error's catalog key and arguments from `IntoResponse` to the
/// translation middleware, via response extensions.
///
/// This exists because the two ends cannot meet any other way: the error knows
/// its code but not the locale, and the middleware knows the locale but sees
/// only a serialized body.
#[derive(Clone, Debug)]
pub struct ErrorPayload {
    pub code: String,
    pub args: Vec<(String, TransArg)>,
}

impl ErrorPayload {
    fn new(code: String, args: Vec<(String, TransArg)>) -> Self {
        ErrorPayload { code, args }
    }

    /// The catalog key this error's message should be looked up under.
    pub fn key(&self) -> String {
        error_key(&self.code)
    }

    /// Arguments in the borrowed shape `Translator::translate` expects.
    pub fn translator_args(&self) -> Vec<(&str, TransArg)> {
        self.args
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect()
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
