use axum::response::{IntoResponse, Response};
use serde::Serialize;

use ferum_application::shared::AppError;

pub mod auth;
pub mod bookmark;
pub mod validators;
pub mod follow;
pub mod category;
pub mod lookup;
pub mod notification;
pub mod post;
pub mod reaction;
pub mod report;
pub mod role;
pub mod search;
pub mod setup;
pub mod tag;
pub mod thread;
pub mod user;
pub mod plugin;
pub mod webhook;
pub mod page_context;

// ─── Shared author info ───────────────────────────────────────────────────────

/// Minimal author info embedded in post/thread responses.
/// `role` is present only for posts (the primary role badge); skip on threads.
#[derive(Serialize)]
pub struct AuthorInfo {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

// ─── Response envelope ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct DataResponse<T: Serialize> {
    pub data: T,
}

impl<T: Serialize> DataResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

#[derive(Serialize)]
pub struct PagedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub meta: PageMeta,
}

#[derive(Serialize)]
pub struct PageMeta {
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

impl<T: Serialize> PagedResponse<T> {
    pub fn new(data: Vec<T>, total: u64, page: u64, per_page: u64) -> Self {
        Self {
            data,
            meta: PageMeta {
                total,
                page,
                per_page,
            },
        }
    }
}

// ─── HandlerError ─────────────────────────────────────────────────────────────

pub struct HandlerError(pub AppError);

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        self.0.into_response()
    }
}

impl From<AppError> for HandlerError {
    fn from(e: AppError) -> Self {
        HandlerError(e)
    }
}

pub type HandlerResult<T> = Result<T, HandlerError>;
