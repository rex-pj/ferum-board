use std::sync::Arc;

use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::app_state::AppState;
use crate::handlers::{api, pages};
use crate::handlers::moderation::api::reports::{create_report, list_my_reports};
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimitConfig};

pub fn public_page_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(pages::forum::home))
        .route("/forum", get(pages::forum::forum_index))
        .route("/forum/{slug}", get(pages::forum::category))
        .route("/forum/t/{slug}", get(pages::forum::thread_detail))
        .route("/catalog", get(pages::catalog::catalog_index))
        .route("/catalog/{slug}", get(pages::catalog::catalog_detail))
        .route("/materials", get(pages::catalog::materials_index))
        .route("/brands", get(pages::catalog::brands_index))
        .route("/go/post/{id}", get(pages::forum::goto_post))
        .route("/sitemap.xml", get(pages::sitemap::sitemap_xml))
        .route("/robots.txt", get(pages::sitemap::robots_txt))
        .route("/u/{username}", get(pages::profile::user_profile))
        .route("/search", get(pages::search::search))
        .route("/setup", get(pages::setup::setup))
        .route("/login", get(pages::auth::login).post(pages::auth::login_post))
        .route("/register", get(pages::auth::register).post(pages::auth::register_post))
        .route("/forgot-password", get(pages::auth::forgot_password).post(pages::auth::forgot_password_post))
        .route("/reset-password", get(pages::auth::reset_password).post(pages::auth::reset_password_post))
        .route("/verify-email/{token}", get(pages::auth::verify_email))
        .route("/account", get(pages::account::account))
        .route("/notifications", get(pages::account::notifications))
        .route("/bookmarks", get(pages::account::bookmarks))
        .route("/new-thread", get(pages::compose::new_thread))
        .route("/edit-thread/{slug}", get(pages::compose::edit_thread))
}

pub fn auth_routes(state: AppState) -> Router<AppState> {
    let auth_rl = Arc::new(RateLimitConfig::auth());
    Router::new()
        .route("/sessions", post(api::auth::login))
        .route("/sessions", delete(api::auth::logout))
        .route("/sessions/refresh", post(api::auth::refresh))
        .route("/registrations", post(api::auth::register))
        .route("/verify-email/{token}", get(api::auth::verify_email))
        .route("/verify-email/resend", post(api::auth::resend_verification))
        .route("/password-resets", post(api::auth::forgot_password))
        .route("/password-resets/{token}", patch(api::auth::reset_password))
        .layer(axum::middleware::from_fn_with_state(
            state,
            rate_limit_middleware,
        ))
        .layer(axum::Extension(auth_rl))
}

pub fn api_routes(state: AppState, write_rl: Arc<RateLimitConfig>) -> Router<AppState> {
    let search_rl = Arc::new(RateLimitConfig::public_write());

    let category_routes = Router::new()
        .route("/", get(api::categories::list_categories))
        .route("/{slug}", get(api::categories::get_category))
        .route("/{slug}/threads", get(api::threads::list_threads))
        .route("/{id}/watch", post(api::categories::watch_category).delete(api::categories::unwatch_category))
        .route("/{id}/mute", post(api::categories::mute_category).delete(api::categories::unmute_category))
        .route("/{id}/watch-status", get(api::categories::get_watch_status));

    let thread_routes = Router::new()
        .route("/", get(api::threads::list_feed).post(api::threads::create_thread))
        .route(
            "/{slug}",
            get(api::threads::get_thread)
                .patch(api::threads::update_thread)
                .delete(api::threads::delete_thread),
        )
        .route("/{slug}/pin", patch(api::threads::pin_thread))
        .route("/{slug}/lock", patch(api::threads::lock_thread))
        .route("/{slug}/move", patch(api::threads::move_thread))
        .route("/{slug}/solve", patch(api::threads::solve_thread))
        .route(
            "/{slug}/posts",
            get(api::posts::list_posts).post(api::posts::create_post),
        )
        .route(
            "/{slug}/bookmarks",
            get(api::bookmarks::get_bookmark_status)
                .post(api::bookmarks::add_bookmark)
                .delete(api::bookmarks::remove_bookmark),
        )
        .route(
            "/{id}/thumbnail",
            post(api::threads::upload_thumbnail).delete(api::threads::delete_thumbnail),
        )
        .route(
            "/{slug}/rating",
            get(api::reviews::get_rating)
                .post(api::reviews::submit_rating)
                .delete(api::reviews::delete_rating),
        )
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    let post_routes = Router::new()
        .route("/attachments", post(api::posts::upload_attachment))
        .route(
            "/{id}",
            patch(api::posts::update_post).delete(api::posts::delete_post),
        )
        .route("/{id}/reactions", post(api::reactions::add_reaction))
        .route("/{id}/reactions/{kind}", delete(api::reactions::remove_reaction))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    let user_routes = Router::new()
        .route("/me", get(api::users::get_me).patch(api::profile::update_profile))
        .route("/me/password", patch(api::profile::change_password))
        .route(
            "/me/avatar",
            post(api::profile::upload_avatar).delete(api::profile::delete_avatar),
        )
        .route(
            "/me/cover",
            post(api::profile::upload_cover).delete(api::profile::delete_cover),
        )
        .route(
            "/me/preferences",
            get(api::profile::get_preferences).put(api::profile::update_preferences),
        )
        .route("/me/bookmarks", get(api::bookmarks::list_bookmarks))
        .route("/{username}", get(api::users::get_public_profile))
        .route("/{username}/threads", get(api::users::list_user_threads))
        .route("/{username}/posts", get(api::users::list_user_posts))
        .route(
            "/{id}/follow",
            post(api::follows::follow_user).delete(api::follows::unfollow_user),
        )
        .route("/{id}/follow-status", get(api::follows::get_follow_status))
        .route("/{id}/followers", get(api::follows::list_followers))
        .route("/{id}/following", get(api::follows::list_following))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    let notification_routes = Router::new()
        .route("/", get(api::notifications::list_notifications))
        .route("/unread-count", get(api::notifications::unread_count))
        .route("/stream", get(api::notifications::sse_stream))
        .route("/read-all", patch(api::notifications::mark_all_read))
        .route("/{id}/read", patch(api::notifications::mark_read));

    let report_routes = Router::new()
        .route("/", post(create_report))
        .route("/mine", get(list_my_reports))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    let search_routes = Router::new()
        .route("/search", get(api::search::search))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(search_rl));

    let tag_routes = Router::new()
        .route("/tags", get(api::tags::list_tags).post(api::tags::create_tag))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    let preview_routes = Router::new()
        .route("/preview-markdown", post(api::posts::preview_markdown))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    // Public catalog reads (no auth, no rate limit — same class as category/thread reads).
    let catalog_routes = Router::new()
        .route("/products", get(api::products::list_products))
        .route("/products/{slug}", get(api::products::get_product))
        .route("/materials", get(api::products::list_materials))
        .route("/brands", get(api::products::list_brands));

    // Member-submitted products (crowd-sourced catalog). Rate-limited like other
    // public writes; the handler enforces `product.submit` + email-verified.
    let catalog_write_routes = Router::new()
        .route("/products", post(api::products::submit_product))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    let plugin_rpc_routes = Router::new()
        .route("/{slug}/rpc/{action}", post(api::plugin_rpc::invoke))
        .route("/{slug}/media", post(api::plugin_rpc::upload_media))
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(axum::Extension(write_rl.clone()));

    Router::new()
        .merge(search_routes)
        .merge(tag_routes)
        .merge(preview_routes)
        .merge(catalog_routes)
        .merge(catalog_write_routes)
        .nest("/categories", category_routes)
        .nest("/threads", thread_routes)
        .nest("/posts", post_routes)
        .nest("/users", user_routes)
        .nest("/notifications", notification_routes)
        .nest("/reports", report_routes)
        .nest("/plugins", plugin_rpc_routes)
}
