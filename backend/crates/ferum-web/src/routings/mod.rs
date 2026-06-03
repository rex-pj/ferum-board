use std::sync::Arc;

use axum::http::Request;
use axum::http::{header, HeaderValue, Method};
use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::middleware::auth::auth_middleware;
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimitConfig};

use crate::app_state::AppState;
use crate::handlers::{
    admin_handler::*,
    admin_stats_handler::*,
    auth_handler::*,
    bookmark_handler::*,
    category_handler::*,
    health_handler::health_handler,
    moderation_handler::*,
    notification_handler::*,
    post_handler::*,
    profile_handler::*,
    reaction_handler::*,
    role_handler::*,
    search_handler::search_handler,
    setup_handler::{run_setup_handler, setup_status_handler},
    site_config_handler::*,
    thread_handler::*,
    upload_handler::serve_upload_handler,
    user_handler::{get_me_handler, get_public_profile_handler, list_user_threads_handler},
    webhook_handler::*,
};

pub fn build_router(state: AppState, cors_origins: &str) -> Router {
    let cors = build_cors(cors_origins);

    let auth_rl = Arc::new(RateLimitConfig::auth());
    let auth_routes = Router::new()
        .route("/sessions", post(login_handler))
        .route("/sessions", delete(logout_handler))
        .route("/refresh", post(refresh_handler))
        .route("/registrations", post(register_handler))
        .route("/verify-email/{token}", get(verify_email_handler))
        .route("/password-resets", post(forgot_password_handler))
        .route("/password-resets/{token}", patch(reset_password_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .layer(axum::Extension(auth_rl));

    let admin_category_routes = Router::new()
        .route(
            "/",
            get(list_categories_handler).post(create_category_handler),
        )
        .route(
            "/{id}",
            patch(update_category_handler).delete(delete_category_handler),
        )
        .route(
            "/{id}/moderators",
            get(list_category_moderators_handler).post(assign_moderator_handler),
        )
        .route(
            "/{id}/moderators/{user_id}",
            delete(revoke_moderator_handler),
        );

    // Extended admin routes
    let admin_user_routes = Router::new()
        .route("/", get(admin_list_users_handler))
        .route("/{id}", get(admin_get_user_handler))
        .route(
            "/{id}/ban",
            post(admin_ban_user_handler).delete(admin_unban_user_handler),
        )
        .route("/{id}/roles", post(assign_user_role_handler))
        .route("/{id}/roles/{role_id}", delete(revoke_user_role_handler));

    let admin_role_routes = Router::new()
        .route("/", get(list_roles_handler).post(create_role_handler))
        .route("/{id}", patch(update_role_handler).delete(delete_role_handler))
        .route(
            "/{id}/permissions",
            get(get_role_permissions_handler).put(set_role_permissions_handler),
        )
        .route("/permissions", get(list_all_permissions_handler));

    let admin_routes = Router::new()
        .route("/stats", get(admin_stats_handler))
        .route("/reports", get(list_admin_reports_handler))
        .route("/audit-log", get(list_audit_log_handler))
        .route(
            "/config",
            get(get_site_config_handler).put(update_site_config_handler),
        )
        .route(
            "/config/logo",
            post(upload_logo_handler).delete(delete_logo_handler),
        )
        .route(
            "/config/favicon",
            post(upload_favicon_handler).delete(delete_favicon_handler),
        )
        .route(
            "/webhooks",
            get(list_webhooks_handler).post(create_webhook_handler),
        )
        .route(
            "/webhooks/{id}",
            patch(update_webhook_handler).delete(delete_webhook_handler),
        )
        .nest("/categories", admin_category_routes)
        .nest("/users", admin_user_routes)
        .nest("/roles", admin_role_routes);

    // Public category routes
    let category_routes = Router::new()
        .route("/", get(list_public_categories_handler))
        .route("/{slug}", get(get_category_handler))
        .route("/{slug}/threads", get(list_threads_handler));

    // Thread routes — the single-segment param is a slug for GET, a UUID for mutations
    let thread_routes = Router::new()
        .route("/", get(list_threads_by_feed).post(create_thread_handler))
        .route(
            "/{slug}",
            get(get_thread_handler)
                .patch(update_thread_handler)
                .delete(delete_thread_handler),
        )
        .route("/{slug}/pin", patch(pin_thread_handler))
        .route("/{slug}/lock", patch(lock_thread_handler))
        .route("/{slug}/move", patch(move_thread_handler))
        .route("/{slug}/solve", patch(solve_thread_handler))
        .route(
            "/{slug}/posts",
            get(list_posts_handler).post(create_post_handler),
        )
        .route(
            "/{slug}/bookmarks",
            get(get_bookmark_status_handler)
                .post(add_bookmark_handler)
                .delete(remove_bookmark_handler),
        )
        .route(
            "/{id}/thumbnail",
            post(upload_thumbnail_handler).delete(delete_thumbnail_handler),
        );

    // Post routes (edit, delete, reactions)
    let post_routes = Router::new()
        .route(
            "/{id}",
            patch(update_post_handler).delete(delete_post_handler),
        )
        .route("/{id}/reactions", post(add_reaction_handler))
        .route("/{id}/reactions/{kind}", delete(remove_reaction_handler));

    // User routes
    let user_routes = Router::new()
        .route("/me", get(get_me_handler).patch(update_profile_handler))
        .route("/me/password", patch(change_password_handler))
        .route(
            "/me/avatar",
            post(upload_avatar_handler).delete(delete_avatar_handler),
        )
        .route(
            "/me/preferences",
            get(get_preferences_handler).put(update_preferences_handler),
        )
        .route("/me/bookmarks", get(list_bookmarks_handler))
        .route("/{username}", get(get_public_profile_handler))
        .route("/{username}/threads", get(list_user_threads_handler));

    // Notification routes
    let notification_routes = Router::new()
        .route("/", get(list_notifications_handler))
        .route("/unread-count", get(unread_count_handler))
        .route("/stream", get(sse_notifications_handler))
        .route("/read-all", patch(mark_all_read_handler))
        .route("/{id}/read", patch(mark_read_handler));

    // Report route (member+)
    let report_routes = Router::new().route("/", post(create_report_handler));

    // Mod routes
    let mod_routes = Router::new()
        .route("/reports", get(list_mod_reports_handler))
        .route("/reports/{id}", patch(resolve_report_handler))
        .route("/users/{id}/warn", post(warn_user_handler))
        .route("/users/{id}/ban", post(temp_ban_handler))
        .route("/audit-log", get(list_mod_audit_log_handler));

    let setup_routes = Router::new()
        .route("/status", get(setup_status_handler))
        .route("/run", post(run_setup_handler));

    Router::new()
        .route("/health", get(health_handler))
        .route("/files/{*key}", get(serve_upload_handler))
        .route("/api/search", get(search_handler))
        .route("/api/public-config", get(get_public_config_handler))
        .route("/api/forum-index", get(get_forum_index_handler))
        .nest("/api/setup", setup_routes)
        .nest("/api/auth", auth_routes)
        .nest("/api/categories", category_routes)
        .nest("/api/threads", thread_routes)
        .nest("/api/posts", post_routes)
        .nest("/api/users", user_routes)
        .nest("/api/notifications", notification_routes)
        .nest("/api/reports", report_routes)
        .nest("/api/mod", mod_routes)
        .nest("/api/admin", admin_routes)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
                tracing::error_span!(
                    "request",
                    method = %req.method(),
                    uri = %req.uri().path(),
                )
            }),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

/// Build a CORS layer from a comma-separated origin list.
/// Wildcard origin ("*" or empty) disables credentials to comply with RFC 6454.
/// An explicit origin list enables credentials (required for httpOnly cookie auth).
fn build_cors(origins: &str) -> CorsLayer {
    let trimmed = origins.trim();
    if trimmed.is_empty() || trimmed == "*" {
        // Wildcard: no credentials — browsers reject the combination anyway.
        return CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any);
        // allow_credentials intentionally omitted (defaults to false)
    }

    let parsed: Vec<HeaderValue> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
        .allow_credentials(true)
}
