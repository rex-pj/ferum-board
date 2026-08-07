use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::tera_engine::TeraEngine;
use ferum_application::ports::{
    CacheService, NotificationSubscriber, PermissionResolver, PluginHookRuntime, PluginRpcRuntime,
    PluginUiRuntime, RateLimiter, StorageService, TokenService, Translator,
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
use ferum_application::usecases::product_usecase::ProductUseCase;
use ferum_application::usecases::reaction_usecase::ReactionUseCase;
use ferum_application::usecases::review_usecase::ReviewUseCase;
use ferum_application::usecases::role_usecase::RoleUseCase;
use ferum_application::usecases::search_usecase::SearchUseCase;
use ferum_application::usecases::setup_usecase::SetupUseCase;
use ferum_application::usecases::tag_usecase::TagUseCase;
use ferum_application::usecases::thread_usecase::ThreadUseCase;
use ferum_application::usecases::user_usecase::UserUseCase;
use ferum_application::usecases::plugin_usecase::PluginUseCase;
use ferum_application::usecases::webhook_usecase::WebhookUseCase;
use ferum_application::usecases::theme_usecase::ThemeUseCase;
use ferum_infrastructure::email::ReloadableEmailService;
use ferum_domain::repositories::{SiteConfigRepository, StoredFileRepository, UserRoleRepository};
use ferum_domain::repositories::user_repository::UserRepository;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    /// The read pool — a clone of `db` unless `DATABASE_READ_URL` is set, in
    /// which case it is a genuinely separate pool against the replica.
    ///
    /// Exists so bulk reads that are not part of rendering a page (today: blob
    /// serving) can be pointed away from the write pool without every caller
    /// having to know whether a replica is configured.
    pub db_read: DatabaseConnection,
    /// Caps how many blob reads may be in flight at once.
    ///
    /// `/files/` loads an entire file into memory through the pool, so without
    /// a cap a burst of image requests takes every connection and the site
    /// stops rendering. This bounds the damage to a slice of the pool: images
    /// queue, pages keep serving. It is a `Semaphore` rather than a bigger pool
    /// because the scarce resource is the database's connections, not ours.
    pub blob_read_permits: Arc<tokio::sync::Semaphore>,
    /// The configured blob backend. Handlers need it for the same two reasons
    /// use cases do: to write bytes, and because it is the only thing that knows
    /// what a file URL looks like under the current configuration.
    pub storage: Arc<dyn StorageService>,
    pub setup: Arc<SetupUseCase>,
    pub auth: Arc<AuthUseCase>,
    pub admin: Arc<AdminUseCase>,
    pub admin_stats: Arc<AdminStatsUseCase>,
    pub bookmark: Arc<BookmarkUseCase>,
    pub follow: Arc<FollowUseCase>,
    pub category: Arc<CategoryUseCase>,
    pub thread: Arc<ThreadUseCase>,
    pub post: Arc<PostUseCase>,
    pub product: Arc<ProductUseCase>,
    pub review: Arc<ReviewUseCase>,
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
    /// Absolute path to the translation catalog root, shown on the admin
    /// languages page so a self-hoster knows where to drop a language pack.
    pub locales_dir: String,
    pub static_dir: String,
    /// Message catalogs. Backs the `t()` Tera function, the error-translation
    /// middleware, and the admin language roster.
    pub translator: Arc<dyn Translator>,
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
    /// The live mail transport, as the concrete reloadable type rather than
    /// `Arc<dyn EmailService>`: the config handler needs `reload()` to apply SMTP
    /// settings edited from /admin/settings without a restart.
    pub email: Arc<ReloadableEmailService>,
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
    /// Resolved once at startup because the CSP now depends on which storage
    /// backend was selected: `img-src` must name the origin `public_url` mints,
    /// or the browser blocks every uploaded image.
    pub security_headers: crate::middleware::security_headers::SecurityHeadersConfig,
    /// Latest answer to "can an anonymous visitor read what we upload?", kept
    /// current by a background task and surfaced on `/health/ready`.
    ///
    /// `None` until the first probe finishes. Cached rather than probed per
    /// request because the question is about bucket IAM, which changes on human
    /// timescales — and because a health endpoint that makes an outbound request
    /// per call is a denial-of-service amplifier.
    pub upload_read_status:
        Arc<tokio::sync::RwLock<Option<ferum_infrastructure::storage::PublicReadProbe>>>,
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
