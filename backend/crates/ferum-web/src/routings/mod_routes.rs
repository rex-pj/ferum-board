use std::sync::Arc;

use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::app_state::AppState;
use crate::handlers::moderation;
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimitConfig};

pub fn mod_api_routes(state: AppState, write_rl: Arc<RateLimitConfig>) -> Router<AppState> {
    // Read-only mod endpoints — no rate limit needed beyond auth.
    let read_routes = Router::new()
        .route("/reports", get(moderation::api::reports::list_reports))
        .route("/audit-log", get(moderation::api::audit_log::list_audit_log))
        .route("/queue", get(moderation::api::queue::list_pending_posts))
        .route("/lookups/users", get(moderation::api::users::lookup_users));

    // State-changing mod actions — rate limited to prevent abuse of warn/ban/approve.
    let write_routes = Router::new()
        .route("/reports/{id}", patch(moderation::api::reports::resolve_report))
        .route("/users/{id}/warn", post(moderation::api::users::warn_user))
        .route("/users/{id}/ban", post(moderation::api::users::temp_ban))
        .route("/queue/{id}/approve", post(moderation::api::queue::approve_post))
        .route("/queue/{id}/reject", delete(moderation::api::queue::reject_post))
        .layer(axum::middleware::from_fn_with_state(state, rate_limit_middleware))
        .layer(axum::Extension(write_rl));

    Router::new().merge(read_routes).merge(write_routes)
}

pub fn mod_page_routes() -> Router<AppState> {
    Router::new()
        .route("/reports", get(moderation::pages::reports::reports))
        .route("/queue", get(moderation::pages::queue::queue))
        .route("/threads", get(moderation::pages::threads::threads))
        .route("/users", get(moderation::pages::users::users))
        .route("/log", get(moderation::pages::log::log))
}
