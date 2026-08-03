use std::sync::Arc;

use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::app_state::AppState;
use crate::handlers::{admin, api};
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimitConfig};

pub fn admin_api_routes() -> Router<AppState> {
    let admin_category_routes = Router::new()
        .route(
            "/",
            get(admin::api::categories::list_categories).post(admin::api::categories::create_category),
        )
        .route(
            "/{id}",
            patch(admin::api::categories::update_category).delete(admin::api::categories::delete_category),
        )
        .route(
            "/{id}/moderators",
            get(admin::api::categories::list_moderators).post(admin::api::categories::assign_moderator),
        )
        .route(
            "/{id}/moderators/{user_id}",
            delete(admin::api::categories::revoke_moderator),
        );

    let admin_user_routes = Router::new()
        .route("/", get(admin::api::users::list_users))
        .route(
            "/{id}",
            get(admin::api::users::get_user).patch(admin::api::users::edit_user),
        )
        .route(
            "/{id}/ban",
            post(admin::api::users::ban_user).delete(admin::api::users::unban_user),
        )
        .route("/{id}/trust-level", patch(admin::api::users::set_trust_level))
        .route("/{id}/unlock", post(admin::api::users::unlock_user))
        .route("/{id}/verify-email", post(admin::api::users::verify_user_email))
        .route("/{id}/roles", post(admin::api::roles::assign_role))
        .route("/{id}/roles/{role_id}", delete(admin::api::roles::revoke_role));

    let admin_role_routes = Router::new()
        .route("/", get(admin::api::roles::list_roles).post(admin::api::roles::create_role))
        .route("/{id}", patch(admin::api::roles::update_role).delete(admin::api::roles::delete_role))
        .route(
            "/{id}/permissions",
            get(admin::api::roles::get_permissions).put(admin::api::roles::set_permissions),
        )
        .route("/permissions", get(admin::api::roles::list_all_permissions));

    let admin_product_routes = Router::new()
        .route(
            "/",
            get(admin::api::products::list_products).post(admin::api::products::create_product),
        )
        .route(
            "/{id}",
            get(admin::api::products::get_product)
                .patch(admin::api::products::update_product)
                .delete(admin::api::products::delete_product),
        )
        .route(
            "/{id}/materials",
            get(admin::api::products::list_product_materials)
                .post(admin::api::products::set_materials),
        )
        .route("/{id}/dependents", get(admin::api::products::product_dependents))
        .route(
            "/{id}/media",
            get(admin::api::products::list_media).post(admin::api::products::upload_media),
        )
        .route("/{id}/media/{media_id}", delete(admin::api::products::delete_media));

    let admin_routes = Router::new()
        .route("/stats", get(admin::api::stats::stats))
        .route("/materials", post(admin::api::products::create_material))
        .route(
            "/materials/{id}",
            patch(admin::api::products::update_material).delete(admin::api::products::delete_material),
        )
        // The catalogue taxonomy. Distinct path from `/categories`, which is the
        // forum's discussion tree — sharing one would be the same conflation
        // this taxonomy exists to undo.
        .route(
            "/product-categories",
            get(admin::api::products::list_product_categories)
                .post(admin::api::products::create_product_category),
        )
        .route(
            "/product-categories/auto-assign",
            post(admin::api::products::auto_assign_product_categories),
        )
        .route(
            "/product-categories/{id}",
            patch(admin::api::products::update_product_category)
                .delete(admin::api::products::delete_product_category),
        )
        .route("/brands", post(admin::api::products::create_brand))
        .route(
            "/brands/{id}",
            patch(admin::api::products::update_brand).delete(admin::api::products::delete_brand),
        )
        .route("/stats/history", get(admin::api::stats::stats_history))
        .route("/reports", get(admin::api::reports::list_reports))
        .route("/audit-log", get(admin::api::reports::list_audit_log))
        .route("/lookups/users", get(admin::api::users::lookup_users))
        .route(
            "/config",
            get(admin::api::config::get_config).put(admin::api::config::update_config),
        )
        .route(
            "/config/logo",
            post(admin::api::config::upload_logo).delete(admin::api::config::delete_logo),
        )
        .route(
            "/config/favicon",
            post(admin::api::config::upload_favicon).delete(admin::api::config::delete_favicon),
        )
        // POST appends one uploaded image; PUT saves captions/links/order and, by
        // omission, deletions. Both live under /config because a hero tile is site
        // configuration, not a content entity with its own lifecycle.
        .route(
            "/config/hero-tiles",
            post(admin::api::config::add_hero_tile).put(admin::api::config::save_hero_tiles),
        )
        .route(
            "/webhooks",
            get(admin::api::webhooks::list_webhooks).post(admin::api::webhooks::create_webhook),
        )
        .route(
            "/webhooks/{id}",
            patch(admin::api::webhooks::update_webhook).delete(admin::api::webhooks::delete_webhook),
        )
        .route("/webhooks/{id}/test", post(admin::api::webhooks::test_webhook))
        .route("/plugins", get(admin::api::plugins::list_plugins))
        .route("/plugins/upload", post(admin::api::plugins::upload_plugin))
        .route("/plugins/install", post(admin::api::plugins::install_plugin))
        .route(
            "/plugins/{slug}",
            get(admin::api::plugins::get_plugin).delete(admin::api::plugins::uninstall_plugin),
        )
        .route("/plugins/{slug}/config", patch(admin::api::plugins::configure_plugin))
        .route("/plugins/{slug}/status", patch(admin::api::plugins::toggle_status))
        .route("/plugins/{slug}/logs", get(admin::api::plugins::get_logs))
        .route("/plugins/{slug}/ui-slots", get(admin::api::plugins::list_ui_slots))
        .route("/plugins/{slug}/ui-slots/{slot_id}", patch(admin::api::plugins::update_ui_slot))
        // Locale roster. The handler lives with the page it backs rather than in
        // api/, since the two share the enabled-locales helpers.
        .route("/languages/{tag}", patch(admin::pages::languages::update_locale))
        .nest("/categories", admin_category_routes)
        .nest("/products", admin_product_routes)
        .nest("/users", admin_user_routes)
        .nest("/roles", admin_role_routes);

    // Debug-only hook test endpoint — excluded from release builds.
    #[cfg(debug_assertions)]
    let admin_routes = admin_routes
        .route("/plugins/debug/hooks", post(admin::api::plugins::debug_hook));

    admin_routes
}

pub fn admin_page_routes() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(admin::pages::dashboard::dashboard))
        .route("/threads", get(admin::pages::threads::threads))
        .route("/users", get(admin::pages::users::users))
        .route("/users/{id}", get(admin::pages::users::user_detail))
        .route("/categories", get(admin::pages::categories::categories))
        .route("/categories/{id}/moderators", get(admin::pages::categories::category_moderators))
        .route("/products", get(admin::pages::products::products))
        .route("/roles", get(admin::pages::roles::roles))
        .route("/roles/{id}", get(admin::pages::roles::role_detail))
        .route("/reports", get(admin::pages::reports::reports))
        .route("/log", get(admin::pages::reports::audit_log))
        .route("/permissions", get(admin::pages::roles::permissions))
        .route("/plugins", get(admin::pages::plugins::plugins))
        .route("/plugins/upload-review", post(admin::pages::plugins::upload_plugin))
        .route("/plugins/install", post(admin::pages::plugins::install_plugin))
        .route("/plugins/{slug}", get(admin::pages::plugins::plugin_detail))
        .route("/plugins/{slug}/activate", post(admin::pages::plugins::activate_plugin))
        .route("/plugins/{slug}/deactivate", post(admin::pages::plugins::deactivate_plugin))
        .route("/plugins/{slug}/uninstall", post(admin::pages::plugins::uninstall_plugin))
        .route("/plugins/{slug}/config", post(admin::pages::plugins::save_config))
        .route("/settings", get(admin::pages::settings::settings))
        .route("/themes", get(admin::pages::themes::themes))
        .route("/languages", get(admin::pages::languages::languages))
        .route("/themes/upload", post(admin::pages::themes::upload_theme))
        .route("/themes/{slug}/activate", post(admin::pages::themes::activate_theme))
        .route("/themes/{slug}/preview", post(admin::pages::themes::upload_theme_preview))
        .route("/themes/{slug}/delete", post(admin::pages::themes::delete_theme))
}

pub fn setup_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(api::setup::status))
        .route("/run", post(api::setup::run))
}

/// `/files/` is unauthenticated and reads whole blobs out of Postgres, which
/// makes it the cheapest way to exhaust the connection pool. It is rate limited
/// for that reason, not to police content access.
pub fn files_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/files/{*key}", get(api::uploads::serve))
        .layer(axum::middleware::from_fn_with_state(state, rate_limit_middleware))
        .layer(axum::Extension(Arc::new(RateLimitConfig::file_read())))
}
