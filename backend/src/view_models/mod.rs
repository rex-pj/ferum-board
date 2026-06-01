use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::application::shared::AppError;

pub mod auth;
pub mod webhook;
pub mod bookmark;
pub mod category;
pub mod notification;
pub mod post;
pub mod reaction;
pub mod report;
pub mod search;
pub mod setup;
pub mod thread;
pub mod user;

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
        Self { data, meta: PageMeta { total, page, per_page } }
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
