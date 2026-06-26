use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::app_state::AppState;
use crate::handlers::moderation;

pub fn mod_api_routes() -> Router<AppState> {
    Router::new()
        .route("/reports", get(moderation::api::reports::list_reports))
        .route("/reports/{id}", patch(moderation::api::reports::resolve_report))
        .route("/users/{id}/warn", post(moderation::api::users::warn_user))
        .route("/users/{id}/ban", post(moderation::api::users::temp_ban))
        .route("/audit-log", get(moderation::api::audit_log::list_audit_log))
        .route("/queue", get(moderation::api::queue::list_pending_posts))
        .route("/queue/{id}/approve", post(moderation::api::queue::approve_post))
        .route("/queue/{id}/reject", delete(moderation::api::queue::reject_post))
}

pub fn mod_page_routes() -> Router<AppState> {
    Router::new()
        .route("/reports", get(moderation::pages::reports::reports))
        .route("/queue", get(moderation::pages::queue::queue))
        .route("/threads", get(moderation::pages::threads::threads))
        .route("/users", get(moderation::pages::users::users))
        .route("/log", get(moderation::pages::log::log))
}
