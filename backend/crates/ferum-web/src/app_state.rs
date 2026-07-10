use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::tera_engine::TeraEngine;
use ferum_application::ports::{
    CacheService, NotificationSubscriber, PermissionResolver, PluginHookRuntime, PluginRpcRuntime,
    PluginUiRuntime, RateLimiter, TokenService,
};
use ferum_application::usecases::admin_stats_usecase::AdminStatsUseCase;
use ferum_application::usecases::admin_usecase::AdminUseCase;
use ferum_application::usecases::auth_usecase::AuthUseCase;
use ferum_application::usecases::bookmark_usecase::BookmarkUseCase;
use ferum_application::usecases::follow_usecase::FollowUseCase;
use ferum_application::usecases::category_usecase::CategoryUseCase;
use ferum_application::usecases::moderation_usecase::ModerationUseCase;
use ferum_application::usecases::notification_usecase::NotificationUseCase;
use ferum_application::usecases::post_usecase::PostUseCase;
use ferum_application::usecases::reaction_usecase::ReactionUseCase;
use ferum_application::usecases::role_usecase::RoleUseCase;
use ferum_application::usecases::search_usecase::SearchUseCase;
use ferum_application::usecases::setup_usecase::SetupUseCase;
use ferum_application::usecases::tag_usecase::TagUseCase;
use ferum_application::usecases::thread_usecase::ThreadUseCase;
use ferum_application::usecases::user_usecase::UserUseCase;
use ferum_application::usecases::plugin_usecase::PluginUseCase;
use ferum_application::usecases::webhook_usecase::WebhookUseCase;
use ferum_application::usecases::theme_usecase::ThemeUseCase;
use ferum_domain::repositories::{SiteConfigRepository, StoredFileRepository, UserRoleRepository};
use ferum_domain::repositories::user_repository::UserRepository;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub setup: Arc<SetupUseCase>,
    pub auth: Arc<AuthUseCase>,
    pub admin: Arc<AdminUseCase>,
    pub admin_stats: Arc<AdminStatsUseCase>,
    pub bookmark: Arc<BookmarkUseCase>,
    pub follow: Arc<FollowUseCase>,
    pub category: Arc<CategoryUseCase>,
    pub thread: Arc<ThreadUseCase>,
    pub post: Arc<PostUseCase>,
    pub reaction: Arc<ReactionUseCase>,
    pub notification: Arc<NotificationUseCase>,
    pub moderation: Arc<ModerationUseCase>,
    pub search: Arc<SearchUseCase>,
    pub user: Arc<UserUseCase>,
    pub role: Arc<RoleUseCase>,
    pub tag: Arc<TagUseCase>,
    pub webhook: Arc<WebhookUseCase>,
    pub plugin: Arc<PluginUseCase>,
    pub theme: Arc<ThemeUseCase>,
    pub tera: TeraEngine,
    pub themes_dir: String,
    pub static_dir: String,
    /// Hook dispatch: used by plugin debug endpoint.
    pub plugin_hooks: Arc<dyn PluginHookRuntime>,
    /// UI slot registry: used by SSR layer for frontend hydration.
    pub plugin_ui: Arc<dyn PluginUiRuntime>,
    /// RPC dispatch: used by the generic /api/plugins/:slug/rpc/:action endpoint.
    pub plugin_rpc: Arc<dyn PluginRpcRuntime>,
    pub site_config: Arc<dyn SiteConfigRepository>,
    /// In-memory authoritative copy of site_config table. Loaded at startup,
    /// updated synchronously on every write — no TTL, no serialization overhead.
    pub site_config_cache: Arc<tokio::sync::RwLock<HashMap<String, String>>>,
    /// In-memory copy of the active theme slug. Loaded at startup, updated on theme activation.
    pub active_theme_cache: Arc<tokio::sync::RwLock<String>>,
    /// Ordered inheritance chain for the active theme: [active, parent, …, "default"].
    /// Used by render_with_theme() to resolve template fallbacks through the hierarchy.
    pub active_theme_chain_cache: Arc<tokio::sync::RwLock<Vec<String>>>,
    /// Bootstrap color scheme for the active theme: "light", "dark", or "auto".
    /// Sourced from theme.json `color_scheme` field; defaults to "auto".
    pub active_theme_color_scheme_cache: Arc<tokio::sync::RwLock<String>>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub user_role_repo: Arc<dyn UserRoleRepository>,
    pub user_repo: Arc<dyn UserRepository>,
    pub permission_resolver: Arc<dyn PermissionResolver>,
    pub token_service: Arc<dyn TokenService>,
    pub cache: Arc<dyn CacheService>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub broadcaster: Arc<dyn NotificationSubscriber>,
    /// True when APP_URL starts with https:// — adds the Secure flag to auth cookies.
    pub cookies_secure: bool,
    /// Absolute site origin (e.g. `https://forum.example.com`, no trailing slash).
    /// Used wherever an absolute URL is required — currently only sitemap.xml.
    pub app_url: String,
    /// Number of trusted reverse proxies. When > 0, X-Forwarded-For is consulted for
    /// rate-limit key derivation. When 0, the raw TCP peer address is always used.
    pub trusted_proxy_count: u32,
    /// Absolute path to the plugins directory on disk.
    pub plugins_dir: String,
    /// Latch flipped to `true` once first-run setup is complete. Lets `setup_guard`
    /// skip a `SELECT COUNT(admins)` DB round-trip on every HTML page load — setup
    /// can never revert to "needed" within a process lifetime.
    pub setup_complete: Arc<std::sync::atomic::AtomicBool>,
}
