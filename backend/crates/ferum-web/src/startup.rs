use std::sync::Arc;

use crate::app_state::AppState;
use crate::config::Config;
use crate::handlers::admin::api::config::smtp_settings_from_config;
use crate::middleware::security_headers::{csp_origin_of, SecurityHeadersConfig};
use crate::tera_engine::TeraEngine;
use ferum_application::event_bus::{EventBus, EventPublisher};
use ferum_application::ports::{
    CacheService, JobQueue, NotificationBus, PermissionResolver, PluginHookRuntime,
    PluginLifecycle, PluginRpcRuntime, PluginUiRuntime, RateLimiter, SearchService, StorageService,
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
use ferum_infrastructure::repositories::PgThemeRepository;
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_domain::repositories::{SiteConfigRepository, ThemeRepository};
#[cfg(feature = "meilisearch")]
use ferum_infrastructure::search::MeilisearchService;
#[cfg(feature = "gcs")]
use ferum_infrastructure::storage::GcsStorageService;
#[cfg(feature = "s3")]
use ferum_infrastructure::storage::S3StorageService;
use ferum_infrastructure::{
    bcrypt_password_hasher::BcryptPasswordHasher,
    cache::{InMemoryCacheService, RedisCacheService},
    email::ReloadableEmailService,
    job_queue::{InlineJobRunner, JobExecutor},
    jwt_token_service::JwtTokenService,
    notification::{SseBroadcaster, SseNotificationBus},
    rate_limit::{InMemoryRateLimiter, NullRateLimiter, RedisRateLimiter},
    plugins::registry::PluginRegistry,
    repositories::{
        PgAuditLogRepository, PgBookmarkRepository, PgCategoryRepository, PgFollowRepository,
        PgBrandRepository, PgMaterialRepository, PgNotificationRepository, PgPermissionRepository,
        PgPluginDbGateway,
        PgPluginRepository, PgPluginStorageRepository, PgPostRepository,
        PgProductCategoryRepository, PgProductRepository,
        PgReactionRepository, PgReportRepository, PgReviewRatingRepository, PgRoleRepository,
        PgSiteConfigRepository, PgStatsRepository, PgStoredFileRepository, PgTagRepository,
        PgThreadRepository, PgUserRepository,
        PgUserRoleRepository, PgWebhookRepository,
    },
    role_permission_cache::RolePermissionCache,
    search::PostgresFtsService,
    storage::{DatabaseStorageService, PublicReadProbe},
    system_seed_service::PgSystemSeedService,
};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

/// How long a plugin's log entries are kept.
///
/// Long enough to investigate a fault reported days after it happened, short
/// enough that a chatty plugin cannot make the table the largest thing in the
/// database. The admin UI paginates these, so older entries have no reader.
const PLUGIN_LOG_RETENTION_DAYS: u32 = 30;

/// How many `/files/` blob reads may run at once.
///
/// Serving a blob reads the whole file into memory over a pooled connection, so
/// the natural assumption is that this caps *connection* use. Measured, that is
/// not what it does. Flooding `/files/` with 120 concurrent requests for a
/// 1.95 MB image, page latency was ~765-800ms whether this was 6 or 500, and
/// whether the pool held 5 connections or 40 — the blob bytes themselves are
/// the bottleneck, not the pool.
///
/// What it does cap is *memory*: the same flood grew the process by 15 MB at 6
/// permits and 74 MB at 500. That is the reason to keep it, and it scales with
/// the per-file limits in `constants.rs`, not with `DB_MAX_CONNECTIONS` — which
/// is also why raising the pool must not widen it.
///
/// The latency result is the stronger argument for the documented production
/// path: serving large blobs out of Postgres degrades under load no matter how
/// it is rationed, so real deployments set `S3_ENDPOINT`.
const BLOB_READ_CONCURRENCY: usize = 6;

/// Server-side ceilings applied to every application connection.
///
/// Until this existed, nothing in the process bounded a query's runtime — a
/// single pathological statement held its connection until the client or the
/// TCP layer gave up, while `acquire_timeout` failed every other caller after
/// five seconds. These are backstops, not tuning: no legitimate request in this
/// application is anywhere near 30s, and a transaction left idle for a minute
/// is a bug holding locks.
///
/// Expressed as libqp `options` rather than a `SET` on checkout because it then
/// applies from the connection's first statement, including ones issued before
/// any repository code runs. `SET LOCAL` inside a transaction still overrides
/// it — that is how the plugin gateway imposes its much tighter 2s ceiling.
const STATEMENT_TIMEOUT_MS: u32 = 30_000;
const IDLE_IN_TRANSACTION_TIMEOUT_MS: u32 = 60_000;

/// Appends the server-side timeouts to a connection URL.
///
/// An operator who has set their own `options=` wins: the parameter can only
/// appear once, and someone who spelled it out explicitly has a reason.
pub fn with_server_timeouts(url: &str) -> String {
    if url.contains("options=") {
        tracing::info!("DATABASE_URL already carries `options=` — leaving server timeouts alone");
        return url.to_string();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    format!(
        "{url}{sep}options=-c%20statement_timeout%3D{STATEMENT_TIMEOUT_MS}%20\
         -c%20idle_in_transaction_session_timeout%3D{IDLE_IN_TRANSACTION_TIMEOUT_MS}"
    )
}

/// Pool options shared by the write and read connections. Pool sizing comes
/// from DB_MAX_CONNECTIONS / DB_MIN_CONNECTIONS; timeouts are fixed.
fn build_connect_options(url: &str, config: &Config) -> ConnectOptions {
    let mut opts = ConnectOptions::new(with_server_timeouts(url));
    opts.max_connections(config.db_max_connections)
        .min_connections(config.db_min_connections)
        .connect_timeout(std::time::Duration::from_secs(5))
        .acquire_timeout(std::time::Duration::from_secs(5))
        .idle_timeout(std::time::Duration::from_secs(300))
        .max_lifetime(std::time::Duration::from_secs(1800))
        // SeaORM logs every statement through `tracing` by default; disable to remove
        // per-query formatting overhead on the hot path. Re-enable with a level filter
        // only when debugging queries.
        .sqlx_logging(false);
    opts
}

pub async fn build_app_state(config: &Config) -> anyhow::Result<AppState> {
    // Observability knobs read by infrastructure without threading through every repo.
    ferum_infrastructure::observability::set_slow_query_threshold_ms(config.slow_query_ms);

    // ─── Run database migrations ─────────────────────────────────────────────
    // On a connection of its own, deliberately WITHOUT the statement timeout the
    // application pool carries. Schema changes are the one legitimate long
    // statement in this process — building a GIN index over a mature `posts`
    // table can run for minutes — and a 30s ceiling would abort the deploy
    // partway through, leaving the schema half-migrated. The connection is
    // dropped as soon as migrations finish, so nothing serving requests inherits
    // the unbounded setting.
    tracing::info!("Running database migrations...");
    {
        let mut migration_opts = ConnectOptions::new(config.database_url.clone());
        migration_opts.max_connections(1).min_connections(1).sqlx_logging(false);
        let migration_conn = Database::connect(migration_opts).await?;
        migration::Migrator::up(&migration_conn, None).await?;
        migration_conn.close().await?;
    }
    tracing::info!("Migrations completed successfully");

    // ─── PostgreSQL ─────────────────────────────────────────────────────────
    let pg_write = Database::connect(build_connect_options(&config.database_url, config)).await?;

    // ─── System data ─────────────────────────────────────────────────────────
    // Idempotent, and deliberately unconditional: migrations carry no data, so
    // this is what guarantees the roles and permissions exist — including any
    // added since this database was created. Must precede
    // RolePermissionCache::load below, which reads exactly these rows.
    PgSystemSeedService::new(pg_write.clone())
        .seed_system()
        .await?;

    let pg_read: DatabaseConnection = match &config.database_read_url {
        Some(url) => {
            tracing::info!("Read replica enabled");
            Database::connect(build_connect_options(url, config)).await?
        }
        None => {
            tracing::info!("No DATABASE_READ_URL — read queries use primary");
            pg_write.clone()
        }
    };

    // ─── Email service ──────────────────────────────────────────────────────
    // Created unconfigured and loaded further down, once site_config is available:
    // SMTP settings are editable from /admin/settings and stored values win over
    // env (env seeds them on first run — see the seeding block below). The
    // reloadable wrapper also owns the "auto-verify registrations" flag, so
    // turning SMTP on or off at runtime takes effect without a restart.
    let email = Arc::new(ReloadableEmailService::new(&config.from_email));

    // ─── SSE broadcaster ─────────────────────────────────────────────────────
    let broadcaster_concrete = Arc::new(SseBroadcaster::new());
    let sse_bus: Arc<dyn NotificationBus> =
        Arc::new(SseNotificationBus::new(broadcaster_concrete.clone()));
    // DIP: web layer holds the trait object, not the concrete SseBroadcaster.
    let broadcaster = broadcaster_concrete as Arc<dyn ferum_application::ports::NotificationSubscriber>;

    // ─── Storage ────────────────────────────────────────────────────────────
    //
    // Presence of the env var is the toggle, as with every other capability.
    // PRECEDENCE, when more than one backend is configured:
    //
    //     GCS_BUCKET  >  S3_ENDPOINT  >  database
    //
    // GCS wins because it is the newer variable: an operator who adds it to a
    // deployment that already had `S3_ENDPOINT` is expressing a new intent, and
    // the reverse order would make that setting appear to do nothing. The loser
    // is named in a WARN rather than ignored silently — a storage backend that
    // is not the one you configured is not something to discover from a missing
    // file weeks later.
    //
    // A backend whose env var is set but whose cargo feature is off is also a
    // WARN, not a silent fall-through to the database, for the same reason.
    // `unused_mut` in the lean build: with neither `gcs` nor `s3` compiled in,
    // both assignments below are cfg'd away and this stays `None`.
    #[allow(unused_mut)]
    let mut storage: Option<Arc<dyn StorageService>> = None;

    if let Some(bucket) = config.gcs_bucket.as_deref() {
        #[cfg(feature = "gcs")]
        {
            tracing::info!("GCS_BUCKET set — using Google Cloud Storage (bucket `{bucket}`)");
            storage = Some(Arc::new(GcsStorageService::new(
                bucket,
                config.gcs_prefix.as_deref(),
                config.cdn_base_url.as_deref(),
                config.gcs_credentials_json.as_deref(),
                config
                    .gcs_credentials_file
                    .as_deref()
                    .or(config.google_application_credentials.as_deref()),
            )?));
        }
        #[cfg(not(feature = "gcs"))]
        tracing::warn!(
            "GCS_BUCKET is set (`{bucket}`) but this binary was built without \
             `--features gcs`; the setting has no effect"
        );
    }

    if let Some(endpoint) = config.s3_endpoint.as_deref() {
        if storage.is_some() {
            tracing::warn!(
                "Both GCS_BUCKET and S3_ENDPOINT are set. GCS wins; S3_ENDPOINT \
                 (`{endpoint}`) is ignored — unset one of them"
            );
        } else {
            #[cfg(feature = "s3")]
            {
                tracing::info!("S3_ENDPOINT set — using S3 object storage");
                storage = Some(Arc::new(
                    S3StorageService::new(
                        endpoint,
                        config.s3_access_key.as_deref().unwrap_or_default(),
                        config.s3_secret_key.as_deref().unwrap_or_default(),
                        config.s3_bucket.as_deref().unwrap_or("forum-uploads"),
                        &config.s3_region,
                        // `None` when unset, NOT the endpoint. Substituting the
                        // endpoint here is what made `public_url` drop the
                        // bucket segment and mint 404s — see `S3StorageService::new`.
                        config.cdn_base_url.as_deref(),
                    )
                    .await,
                ));
            }
            #[cfg(not(feature = "s3"))]
            tracing::warn!(
                "S3_ENDPOINT is set (`{endpoint}`) but this binary was built without \
                 `--features s3`; the setting has no effect"
            );
        }
    }

    // Recorded here, where the answer is a fact rather than an inference. Post
    // attachments stage in the database and are promoted outward on publish, and
    // that must happen only when there is genuinely somewhere else to put them.
    // Deriving it later from a `public_url` shape would misread database storage
    // behind `CDN_BASE_URL` as an object store and delete the bytes.
    let uses_object_store = storage.is_some();

    let storage: Arc<dyn StorageService> = match storage {
        Some(backend) => backend,
        None => {
            tracing::info!("No object store configured — using database storage");
            // `CDN_BASE_URL` is honoured here too: a pull-CDN pointed at this app
            // serves `/files/` perfectly well. The previous `#[cfg(not(s3))]`
            // arm dropped this call, so a build without the s3 feature silently
            // ignored the variable.
            Arc::new(
                DatabaseStorageService::new(pg_write.clone())
                    .with_cdn_base_url(config.cdn_base_url.as_deref()),
            )
        }
    };

    // ─── Repositories ────────────────────────────────────────────────────────
    let user_repo = Arc::new(PgUserRepository::new(pg_write.clone()));
    let role_repo = Arc::new(PgRoleRepository::new(pg_write.clone()));
    let permission_repo = Arc::new(PgPermissionRepository::new(pg_write.clone()));
    let user_role_repo = Arc::new(PgUserRoleRepository::new(pg_write.clone()));
    let category_repo = Arc::new(PgCategoryRepository::new(pg_write.clone()));
    let thread_repo = Arc::new(PgThreadRepository::new(pg_write.clone()));
    let post_repo = Arc::new(PgPostRepository::new(pg_write.clone()));
    let reaction_repo = Arc::new(PgReactionRepository::new(pg_write.clone()));
    let notification_repo = Arc::new(PgNotificationRepository::new(pg_write.clone()));
    let report_repo = Arc::new(PgReportRepository::new(pg_write.clone()));
    let audit_log_repo = Arc::new(PgAuditLogRepository::new(pg_write.clone()));
    let bookmark_repo = Arc::new(PgBookmarkRepository::new(pg_write.clone()));
    let webhook_repo = Arc::new(PgWebhookRepository::new(pg_write.clone()));
    let stored_file_repo: Arc<dyn ferum_domain::repositories::StoredFileRepository> =
        Arc::new(PgStoredFileRepository::new(pg_write.clone()));
    let tag_repo: Arc<dyn ferum_domain::repositories::TagRepository> =
        Arc::new(PgTagRepository::new(pg_write.clone()));
    let product_repo: Arc<dyn ferum_domain::repositories::product_repository::ProductRepository> =
        Arc::new(PgProductRepository::new(pg_write.clone()));
    let product_category_repo: Arc<
        dyn ferum_domain::repositories::product_repository::ProductCategoryRepository,
    > = Arc::new(PgProductCategoryRepository::new(pg_write.clone()));
    let material_repo: Arc<dyn ferum_domain::repositories::material_repository::MaterialRepository> =
        Arc::new(PgMaterialRepository::new(pg_write.clone()));
    let brand_repo: Arc<dyn ferum_domain::repositories::brand_repository::BrandRepository> =
        Arc::new(PgBrandRepository::new(pg_write.clone()));
    let review_rating_repo: Arc<
        dyn ferum_domain::repositories::review_rating_repository::ReviewRatingRepository,
    > = Arc::new(PgReviewRatingRepository::new(pg_write.clone()));

    // ─── RolePermissionCache (in-memory, loaded from DB after migrations) ────
    let role_permission_cache = Arc::new(RolePermissionCache::new(pg_write.clone()));
    role_permission_cache.load().await?;
    tracing::info!("Role permission cache loaded");
    // DIP: web layer holds the trait object, not the concrete RolePermissionCache.
    let permission_resolver: Arc<dyn PermissionResolver> = role_permission_cache;

    // ─── Search ──────────────────────────────────────────────────────────────
    #[cfg(feature = "meilisearch")]
    let search_svc: Arc<dyn SearchService> = match &config.meilisearch_url {
        Some(url) => {
            tracing::info!("MEILISEARCH_URL set — using Meilisearch");
            Arc::new(MeilisearchService::new(
                url,
                config.meilisearch_key.as_deref(),
                &config.meilisearch_index,
                &config.meilisearch_product_index,
            ))
        }
        None => Arc::new(PostgresFtsService::new(pg_read.clone())),
    };
    #[cfg(not(feature = "meilisearch"))]
    let search_svc: Arc<dyn SearchService> = Arc::new(PostgresFtsService::new(pg_read.clone()));

    // Site name is interpolated into transactional email copy. Read once here
    // rather than per-send: the job worker runs detached from any request and has
    // no access to the live config cache, and a site rename is rare enough that
    // picking it up on the next restart is acceptable.
    let site_name = PgSiteConfigRepository::new(pg_write.clone())
        .get("site_name")
        .await
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ferum_application::constants::DEFAULT_SITE_NAME.to_string());

    // ─── Translation catalogs ─────────────────────────────────────────────────
    // Built early because the job executor below needs it: transactional emails
    // are written in the recipient's language, and the executor is constructed
    // as part of the Redis/in-memory branch.
    //
    // Roots are listed in ascending precedence: a theme's catalog shadows core,
    // the same direction the theme template chain resolves.
    //
    // Both candidate paths are tried because the app is normally run from
    // `backend/` (so `../locales`) but the compiled binary may be run from the
    // repository root (so `./locales`) — the same split THEMES_DIR has.
    let locales_dir = {
        let configured = std::path::PathBuf::from(&config.locales_dir);
        let candidates = [
            configured.clone(),
            std::path::PathBuf::from("./locales"),
            std::path::PathBuf::from("../locales"),
        ];
        match candidates.iter().find(|p| p.is_dir()) {
            Some(found) => {
                if found != &configured {
                    tracing::warn!(
                        configured = %configured.display(),
                        using = %found.display(),
                        "LOCALES_DIR not found; using a fallback path"
                    );
                }
                found.clone()
            }
            None => anyhow::bail!(
                "translation catalogs not found (tried {}). Every page renders its \
                 message keys without them — set LOCALES_DIR.",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    };
    // Themes contribute their own catalogs from their own tree, listed after core
    // so a theme can override a core string as well as add its own — the same
    // precedence direction the theme template chain resolves in.
    let mut catalog_roots = vec![locales_dir];
    catalog_roots.extend(theme_locale_roots(&config.themes_dir));

    let translator: Arc<dyn ferum_application::ports::Translator> =
        Arc::new(ferum_infrastructure::i18n::FluentTranslator::new(catalog_roots).await);

    // Fail closed on an empty catalog, the same policy first-party templates get.
    //
    // This was originally a warning, on the reasoning that rendering keys is
    // "ugly but still serves pages". That was wrong: it shipped a forum whose
    // navigation read `ui-home` / `ui-categories`, and nothing surfaced it until
    // someone looked at a screenshot. A site in that state is broken, not degraded.
    let key_count = translator.default_locale_keys().len();
    anyhow::ensure!(
        key_count > 0,
        "translation catalog at {} loaded zero messages for the default locale — \
         every page would render raw message keys. Check that the directory \
         contains a `{}/` subdirectory with .ftl files.",
        config.locales_dir,
        ferum_domain::Locale::DEFAULT_TAG,
    );

    tracing::info!(
        locales = ?translator.available_locales().iter().map(|l| l.to_string()).collect::<Vec<_>>(),
        messages = key_count,
        "translation catalogs ready"
    );

    // ─── Redis or in-memory fallbacks ────────────────────────────────────────
    // Named because both arms of the match below must spell it out, and the
    // bare tuple tripped clippy::type_complexity.
    type RedisBackedPorts = (
        Arc<dyn CacheService>,
        Arc<dyn RateLimiter>,
        Arc<dyn JobQueue>,
        Arc<dyn NotificationBus>,
    );
    let (cache, rate_limiter_raw, job_queue, notification_bus): RedisBackedPorts =
        match &config.redis_url {
        Some(url) => {
            tracing::info!("REDIS_URL set — using Redis cache and rate limiter");
            let executor = Arc::new(JobExecutor::new(
                email.clone(),
                config.app_url.clone(),
                storage.clone(),
                stored_file_repo.clone(),
                webhook_repo.clone(),
            )
            .with_translator(Arc::clone(&translator), site_name.clone()));
            let cache = RedisCacheService::new(url)
                .await
                .map(|s| -> Arc<dyn CacheService> { Arc::new(s) })
                .unwrap_or_else(|e| {
                    tracing::warn!("Redis cache connect failed ({}), falling back to in-memory", e);
                    Arc::new(InMemoryCacheService::new())
                });
            let rate_limiter = RedisRateLimiter::new(url)
                .await
                .map(|s| -> Arc<dyn RateLimiter> { Arc::new(s) })
                .unwrap_or_else(|e| {
                    tracing::warn!("Redis rate limiter connect failed ({}), falling back to in-memory", e);
                    Arc::new(InMemoryRateLimiter::new())
                });
            (cache, rate_limiter, Arc::new(InlineJobRunner::new(executor)), sse_bus)
        }
        None => {
            tracing::warn!("REDIS_URL not set — in-memory fallbacks (single-instance only)");
            let executor = Arc::new(JobExecutor::new(
                email.clone(),
                config.app_url.clone(),
                storage.clone(),
                stored_file_repo.clone(),
                webhook_repo.clone(),
            )
            .with_translator(Arc::clone(&translator), site_name.clone()));
            (
                Arc::new(InMemoryCacheService::new()),
                Arc::new(InMemoryRateLimiter::new()),
                Arc::new(InlineJobRunner::new(executor)),
                sse_bus,
            )
        }
    };

    let rate_limiter: Arc<dyn RateLimiter> = if config.rate_limit_enabled {
        rate_limiter_raw
    } else {
        tracing::warn!("Rate limiting DISABLED (RATE_LIMIT_ENABLED=false)");
        Arc::new(NullRateLimiter)
    };

    // ─── Infrastructure ───────────────────────────────────────────────────────
    if config.jwt_secret.len() < 32 {
        anyhow::bail!(
            "JWT_SECRET must be at least 32 characters (got {}). \
             Generate one with: openssl rand -hex 32",
            config.jwt_secret.len()
        );
    }
    let hasher = Arc::new(BcryptPasswordHasher);
    let token_service: Arc<dyn ferum_application::ports::TokenService> =
        Arc::new(JwtTokenService::new(
            &config.jwt_secret,
            config.jwt_expiry_seconds,
            config.refresh_token_expiry_days * 86_400,
        ));

    // ─── Plugin system (before event_bus so plugin_runtime can be injected) ──
    let plugin_repo = Arc::new(PgPluginRepository::new(pg_write.clone()));
    let plugin_storage_repo = Arc::new(PgPluginStorageRepository::new(pg_write.clone()));
    let plugin_db_gateway = Arc::new(PgPluginDbGateway::new(pg_write.clone()));
    let plugin_registry = Arc::new(PluginRegistry::new(
        plugin_repo.clone(),
        cache.clone(),
        plugin_storage_repo,
        user_repo.clone(),
        notification_repo.clone(),
        plugin_db_gateway.clone(),
        config.plugin_hook_timeout_ms,
        config.plugin_circuit_threshold,
    ));
    if let Err(e) = plugin_registry.load_from_db().await {
        tracing::warn!("Plugin registry failed to load from DB: {:?}", e);
    }
    // ISP: cast once to each focused trait so each consumer receives only the interface it needs.
    let plugin_hooks: Arc<dyn PluginHookRuntime> = plugin_registry.clone();
    let plugin_ui: Arc<dyn PluginUiRuntime> = plugin_registry.clone();
    let plugin_rpc: Arc<dyn PluginRpcRuntime> = plugin_registry.clone();
    let plugin_lifecycle: Arc<dyn PluginLifecycle> = plugin_registry;

    let event_bus: Arc<dyn EventPublisher> = Arc::new(
        EventBus::new(
            audit_log_repo,
            notification_repo.clone(),
            notification_bus.clone(),
            webhook_repo.clone(),
            job_queue.clone(),
        )
        .with_plugin_runtime(plugin_hooks.clone()),
    );

    // ─── Site config (before use cases — they read from it) ──────────────────
    let site_config: Arc<dyn SiteConfigRepository> =
        Arc::new(PgSiteConfigRepository::new(pg_write.clone()));

    // Seed SMTP credentials from env into site_config on first run so admins can
    // later update them via the settings UI without touching env vars.
    //
    // Only when the key is *absent*. A stored empty host means an admin cleared
    // SMTP deliberately; re-seeding it from env would undo that on every restart,
    // and site_config is the authority now that the settings page writes here.
    if config.smtp_host.is_some() {
        let existing_smtp = site_config.get("smtp_host").await.unwrap_or(None);
        if existing_smtp.is_none() {
            let mut smtp_seed = std::collections::HashMap::new();
            if let Some(h) = &config.smtp_host {
                smtp_seed.insert("smtp_host".to_string(), h.clone());
            }
            smtp_seed.insert("smtp_port".to_string(), config.smtp_port.to_string());
            if let Some(u) = &config.smtp_user {
                smtp_seed.insert("smtp_user".to_string(), u.clone());
            }
            if let Some(p) = &config.smtp_pass {
                smtp_seed.insert("smtp_pass".to_string(), p.clone());
            }
            if let Err(e) = site_config.set_many(&smtp_seed).await {
                tracing::warn!("Failed to seed SMTP config from env: {e}");
            }
        }
    }

    let site_config_cache = Arc::new(tokio::sync::RwLock::new(
        site_config.get_all().await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load site config at startup: {e}, using defaults");
            std::collections::HashMap::new()
        }),
    ));

    // ─── Mail transport ─────────────────────────────────────────────────────
    // Loaded from site_config (seeded from env above), so an operator who changed
    // SMTP through the admin UI keeps those settings across restarts. A bad stored
    // value must not stop the process booting — log and leave email disabled,
    // which also flips the auto-verify flag so registration still works.
    {
        let stored = site_config_cache.read().await.clone();
        let settings = match smtp_settings_from_config(&stored) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Stored SMTP settings are invalid ({e}) — email disabled");
                None
            }
        };
        let (host, port, user, pass) = match &settings {
            Some(s) => (
                Some(s.host.as_str()),
                s.port,
                s.username.as_deref(),
                s.password.as_deref(),
            ),
            None => (None, config.smtp_port, None, None),
        };
        if let Err(e) = email.reload(host, port, user, pass).await {
            tracing::error!("Failed to build SMTP transport ({e}) — email disabled");
        }
    }

    // ─── Use cases ───────────────────────────────────────────────────────────
    let auth = Arc::new(
        AuthUseCase::new(
            user_repo.clone(),
            role_repo.clone(),
            user_role_repo.clone(),
            hasher,
            token_service.clone(),
            cache.clone(),
            job_queue.clone(),
        )
        // Shared flag, not a snapshot: it must follow SMTP being configured or
        // cleared at runtime, or registrations keep skipping email verification
        // long after mail started working.
        .with_auto_verify_flag(email.auto_verify_flag())
        .with_site_config(site_config.clone())
        .with_plugin_runtime(plugin_hooks.clone()),
    );

    let admin = Arc::new(
        AdminUseCase::new(
            category_repo.clone(),
            role_repo.clone(),
            user_role_repo.clone(),
            user_repo.clone(),
            Arc::new(PgAuditLogRepository::new(pg_write.clone())),
            cache.clone(),
        )
        .with_plugin_runtime(plugin_hooks.clone())
        .with_permissions(permission_repo.clone()),
    );

    let role = Arc::new(RoleUseCase::new(
        role_repo.clone(),
        permission_repo.clone(),
        user_role_repo.clone(),
    ));

    let category = Arc::new(
        CategoryUseCase::new(category_repo.clone(), thread_repo.clone(), tag_repo.clone(), user_repo.clone())
            .with_site_config(site_config.clone())
            .with_cache(cache.clone()),
    );

    let thread = Arc::new(
        ThreadUseCase::new(
            thread_repo.clone(),
            category_repo.clone(),
            post_repo.clone(),
            job_queue.clone(),
            stored_file_repo.clone(),
            storage.clone(),
            event_bus.clone(),
            cache.clone(),
            tag_repo.clone(),
            user_repo.clone(),
        )
        .with_dedup_view_counts(config.dedup_view_counts)
        .with_site_config(site_config.clone())
        .with_plugin_runtime(plugin_hooks.clone()),
    );

    // ─── Theme slug cache ────────────────────────────────────────────────────────
    let theme_repo_for_cache = Arc::new(PgThemeRepository::new(pg_write.clone()));
    let initial_active_slug = theme_repo_for_cache
        .get_active()
        .await
        .map(|t| t.slug)
        .unwrap_or_else(|_| ferum_application::constants::DEFAULT_THEME_SLUG.to_string());
    let active_theme_cache = Arc::new(tokio::sync::RwLock::new(initial_active_slug.clone()));

    let post = Arc::new(
        PostUseCase::new(
            post_repo.clone(),
            thread_repo.clone(),
            category_repo.clone(),
            user_repo.clone(),
            reaction_repo.clone(),
            site_config.clone(),
            event_bus.clone(),
        )
        .with_plugin_runtime(plugin_hooks.clone())
        // Third argument is staging: database-backed, and present ONLY when an
        // object store is actually configured. A staged attachment is authorized
        // per viewer, which cannot be enforced once its bytes are in a public
        // bucket, so they move outward only when a post publishes them. With no
        // object store there is nowhere to move them to, and `None` says so.
        .with_stored_files(
            stored_file_repo.clone(),
            storage.clone(),
            uses_object_store
                .then(|| Arc::new(DatabaseStorageService::new(pg_write.clone())) as Arc<dyn StorageService>),
        ),
    );

    let reaction = Arc::new(
        ReactionUseCase::new(
            reaction_repo.clone(),
            post_repo.clone(),
            thread_repo.clone(),
            category_repo.clone(),
            user_repo.clone(),
            event_bus.clone(),
        )
        .with_plugin_runtime(plugin_hooks.clone()),
    );

    let notification = Arc::new(NotificationUseCase::new(notification_repo.clone()));

    let product = Arc::new(ProductUseCase::new(
        product_repo.clone(),
        product_category_repo,
        material_repo,
        brand_repo,
        stored_file_repo.clone(),
        storage.clone(),
        job_queue.clone(),
    ));
    let review = Arc::new(ReviewUseCase::new(review_rating_repo));

    let moderation = Arc::new(
        ModerationUseCase::new(
            report_repo,
            post_repo.clone(),
            thread_repo.clone(),
            user_repo.clone(),
            notification_repo.clone(),
            Arc::new(PgAuditLogRepository::new(pg_write.clone())),
            event_bus.clone(),
            cache.clone(),
        )
        .with_plugin_runtime(plugin_hooks.clone())
        .with_staff_lookup(user_role_repo.clone(), permission_resolver.clone()),
    );

    let search = Arc::new(SearchUseCase::new(
        search_svc,
        thread_repo.clone(),
        category_repo.clone(),
        product_repo,
    ));

    let stats_repo = Arc::new(PgStatsRepository::new(pg_write.clone()));
    let admin_stats = Arc::new(AdminStatsUseCase::new(stats_repo).with_cache(cache.clone()));

    let hasher2 = Arc::new(BcryptPasswordHasher);
    let user = Arc::new(
        UserUseCase::new(
            user_repo.clone(),
            hasher2,
            stored_file_repo.clone(),
            storage.clone(),
            job_queue.clone(),
        )
        .with_cache(cache.clone()),
    );

    let bookmark = Arc::new(BookmarkUseCase::new(
        bookmark_repo,
        thread_repo.clone(),
        category_repo.clone(),
    ));
    let follow_repo = Arc::new(PgFollowRepository::new(pg_write.clone()));
    let follow = Arc::new(FollowUseCase::new(
        follow_repo,
        user_repo.clone(),
        event_bus.clone(),
    ));
    let tag = Arc::new(TagUseCase::new(tag_repo.clone()));
    let webhook = Arc::new(WebhookUseCase::new(
        webhook_repo.clone(),
        Arc::new(ferum_infrastructure::webhook_delivery::ReqwestWebhookDeliveryService),
        Arc::new(ferum_infrastructure::network_utils::TokioHostResolver),
    ));
    // Cloned before the use case takes ownership — the log-retention task below
    // needs the repository directly, not through a use case that would require
    // an `AuthUser` for a job with no actor.
    let plugin_repo_for_prune = plugin_repo.clone();
    let plugin = Arc::new(PluginUseCase::new(
        plugin_repo,
        webhook_repo.clone(),
        plugin_lifecycle,
        plugin_db_gateway,
        stored_file_repo.clone(),
        storage.clone(),
        job_queue.clone(),
        std::path::PathBuf::from(&config.plugins_dir),
    ));

    let hasher3 = Arc::new(BcryptPasswordHasher);
    // Same adapter/fallback shape as storage, search and the plugin runtime: the
    // use case only ever sees `Arc<dyn BulkSeedService>`, so compiling the
    // 1,400-line example dataset out changes nothing above this line.
    #[cfg(feature = "bulk_seed")]
    let bulk_seed: Arc<dyn ferum_application::ports::BulkSeedService> = Arc::new(
        ferum_infrastructure::bulk_seed_service::PgBulkSeedService::new(pg_write.clone()),
    );
    #[cfg(not(feature = "bulk_seed"))]
    let bulk_seed: Arc<dyn ferum_application::ports::BulkSeedService> =
        Arc::new(ferum_application::ports::NullBulkSeedService);
    let setup = Arc::new(SetupUseCase::new(
        user_repo.clone(),
        role_repo,
        user_role_repo.clone(),
        hasher3,
        token_service.clone(),
        cache.clone(),
        site_config.clone(),
        bulk_seed,
    ));

    // ─── Theme system ────────────────────────────────────────────────────────
    let theme_repo = Arc::new(PgThemeRepository::new(pg_write.clone()));
    let theme = Arc::new(ThemeUseCase::new(theme_repo));

    let initial_chain = theme
        .resolve_chain(&initial_active_slug)
        .await;
    let active_theme_chain_cache = Arc::new(tokio::sync::RwLock::new(initial_chain));

    let initial_color_scheme = read_theme_color_scheme(&config.themes_dir, &initial_active_slug);
    let active_theme_color_scheme_cache = Arc::new(tokio::sync::RwLock::new(initial_color_scheme));

    // Two distinct failures used to be conflated here. A *misconfigured path* is
    // recoverable — retry the repo-relative default. A *broken template* is not:
    // retrying it just parses the same bad file again, and if the fallback path
    // happens not to exist, `build_tera` finds zero templates, returns Ok, and the
    // app boots serving 500s from every page. So resolve the directories first,
    // then let a parse error abort startup.
    let (themes_dir, admin_templates_dir, static_dir) = {
        let configured = (
            std::path::PathBuf::from(&config.themes_dir),
            std::path::PathBuf::from(&config.admin_templates_dir),
            std::path::PathBuf::from(&config.static_dir),
        );
        if configured.0.is_dir() && configured.1.is_dir() {
            configured
        } else {
            tracing::warn!(
                themes_dir = %configured.0.display(),
                admin_templates_dir = %configured.1.display(),
                "configured template directories not found, falling back to ./frontend/*"
            );
            (
                std::path::PathBuf::from("./frontend/themes"),
                std::path::PathBuf::from("./frontend/templates"),
                std::path::PathBuf::from("./frontend/static"),
            )
        }
    };
    anyhow::ensure!(
        themes_dir.is_dir() && admin_templates_dir.is_dir(),
        "template directories not found: themes={}, templates={} (set THEMES_DIR / ADMIN_TEMPLATES_DIR)",
        themes_dir.display(),
        admin_templates_dir.display(),
    );
    // Propagates on a broken first-party template — refusing to boot beats booting
    // into a forum whose admin panel 500s. User themes still fail open inside.
    let tera = TeraEngine::new(
        themes_dir,
        admin_templates_dir,
        static_dir,
        Arc::clone(&translator),
    )
    .map_err(|e| anyhow::anyhow!("template load failed: {e}"))?;

    // ─── Background: flush daily stats every 5 minutes ─────────────────────────
    // One-time backfill: populate daily_stats for all past dates from source tables.
    // Idempotent (ON CONFLICT DO NOTHING) — safe to run on every restart.
    if let Err(e) = admin_stats.backfill_history().await {
        tracing::warn!("daily_stats backfill failed: {:?}", e);
    }
    // Flush today's data immediately so chart is up-to-date from the first request.
    if let Err(e) = admin_stats.flush_daily_stats().await {
        tracing::warn!("startup daily_stats flush failed: {:?}", e);
    }
    {
        let stats_uc = admin_stats.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                if let Err(e) = stats_uc.flush_daily_stats().await {
                    tracing::warn!("daily_stats flush failed: {:?}", e);
                }
            }
        });
    }

    // ─── Background: flush view count buffer every 60s ─────────────────────────
    // Batches threads.view_count UPDATE to avoid hot-row contention under load.
    {
        let thread_uc = thread.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Err(e) = thread_uc.flush_view_counts().await {
                    tracing::warn!("view count flush failed: {:?}", e);
                }
            }
        });
    }

    // ─── Background: prune plugin_logs daily ───────────────────────────────────
    // `plugin_logs` is written at a rate plugin authors control and had no
    // retention at all — `delete_old_logs` existed, was tested, and was never
    // called from anywhere, so the table grew without bound for the life of an
    // install. Runs once at startup and then daily; a miss is harmless, so
    // failures are logged rather than retried.
    {
        let repo = plugin_repo_for_prune;
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
            loop {
                interval.tick().await;
                match repo.delete_old_logs(PLUGIN_LOG_RETENTION_DAYS).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(deleted = n, "pruned plugin logs"),
                    Err(e) => tracing::warn!("plugin log prune failed: {:?}", e),
                }
            }
        });
    }

    // ─── Background: prune expired notifications daily ─────────────────────────
    // Shares the plugin-log job's shape and reasoning: a table written on the
    // request path with no retention grows for the life of the install, and here
    // it also inflates the inbox COUNT that every visit to /notifications pays.
    {
        let repo = notification_repo.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
            loop {
                interval.tick().await;
                match repo
                    .delete_expired(
                        ferum_application::constants::NOTIFICATION_READ_RETENTION_DAYS,
                        ferum_application::constants::NOTIFICATION_UNREAD_RETENTION_DAYS,
                    )
                    .await
                {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(deleted = n, "pruned expired notifications"),
                    Err(e) => tracing::warn!("notification prune failed: {:?}", e),
                }
            }
        });
    }

    let cookies_secure = config.app_url.starts_with("https://");

    // Derived from the backend that was actually selected, by asking it for a
    // URL, rather than re-deriving it from the same env vars the selection block
    // above already read. Re-deriving would be a second copy of the precedence
    // rules and the feature gates, free to drift — and drift here is invisible
    // from the server: images simply stop rendering in the browser.
    let image_origins: Vec<String> =
        csp_origin_of(&storage.public_url("csp-probe")).into_iter().collect();
    if let Some(origin) = image_origins.first() {
        tracing::info!("Uploads are served from {origin} — added to the CSP `img-src` allowlist");
    }
    let security_headers = SecurityHeadersConfig::new(cookies_secure, &image_origins);

    // Same question from the other side: the CSP now *permits* that origin, but
    // can a visitor actually read from it? Spawned rather than awaited — the
    // answer is advisory, and binding process start to an outbound request would
    // turn a slow object store into a failed deploy.
    let upload_read_status = Arc::new(tokio::sync::RwLock::new(None));
    spawn_public_read_probe(storage.clone(), upload_read_status.clone());

    warn_degraded_capabilities(config);

    Ok(AppState {
        db: pg_write.clone(),
        db_read: pg_read.clone(),
        blob_read_permits: Arc::new(tokio::sync::Semaphore::new(BLOB_READ_CONCURRENCY)),
        storage: storage.clone(),
        setup,
        auth,
        admin,
        admin_stats,
        bookmark,
        follow,
        category,
        thread,
        post,
        product,
        review,
        reaction,
        notification,
        moderation,
        search,
        user,
        role,
        tag,
        webhook,
        plugin,
        plugin_hooks,
        plugin_ui,
        plugin_rpc,
        site_config,
        site_config_cache,
        email: email.clone(),
        active_theme_cache,
        active_theme_chain_cache,
        active_theme_color_scheme_cache,
        stored_files: stored_file_repo,
        user_role_repo,
        user_repo,
        permission_resolver,
        token_service,
        cache,
        rate_limiter,
        broadcaster,
        theme,
        tera,
        themes_dir: config.themes_dir.clone(),
        locales_dir: config.locales_dir.clone(),
        static_dir: config.static_dir.clone(),
        translator,
        cookies_secure,
        security_headers,
        upload_read_status,
        app_url: config.app_url.trim_end_matches('/').to_string(),
        trusted_proxy_count: config.trusted_proxy_count,
        plugins_dir: config.plugins_dir.clone(),
        setup_complete: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

pub async fn maybe_run_headless_setup(config: &Config, state: &AppState) -> anyhow::Result<()> {
    let (Some(username), Some(email), Some(password)) = (
        config.setup_admin_username.as_deref(),
        config.setup_admin_email.as_deref(),
        config.setup_admin_password.as_deref(),
    ) else {
        return Ok(());
    };

    if !state.setup.needs_setup().await? {
        return Ok(());
    }

    tracing::info!("SETUP_ADMIN_* env vars detected — running headless first-run setup");

    state
        .setup
        .run_setup(ferum_application::usecases::setup_usecase::RunSetupCmd {
            admin_username: username.to_string(),
            admin_email: email.to_string(),
            admin_password: password.to_string(),
            config: None,
            seed_example_data: false,
        })
        .await?;

    tracing::info!("Headless setup complete — admin account created for '{}'", username);
    Ok(())
}

/// How often the anonymous-read check is repeated.
///
/// What it detects is a change to bucket IAM, which happens on human timescales,
/// so this is about noticing within the hour rather than within the second. Each
/// tick is one HTTP request; anything much shorter would be paying continuously
/// to watch something that rarely moves.
const PUBLIC_READ_RECHECK: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Keeps `upload_read_status` current with whether an anonymous visitor can read
/// what this deployment uploads.
///
/// A private bucket is invisible from the server — uploads succeed, rows are
/// written, logs stay clean — and shows up only as broken images in somebody's
/// browser. Checking once at startup caught the deploy that got it wrong; it did
/// not catch permissions being changed on a running bucket, which is why this
/// repeats.
///
/// Only *transitions* are logged. A WARN repeated every fifteen minutes forever
/// stops being read, and the state is on `/health/ready` for anyone who wants to
/// poll it.
fn spawn_public_read_probe(
    storage: Arc<dyn StorageService>,
    status: Arc<tokio::sync::RwLock<Option<PublicReadProbe>>>,
) {
    tokio::spawn(async move {
        let mut previous: Option<&'static str> = None;
        loop {
            let outcome = ferum_infrastructure::storage::probe_public_read(storage.as_ref()).await;
            let label = outcome.label();
            let changed = previous != Some(label);
            previous = Some(label);
            let same_origin = matches!(outcome, PublicReadProbe::SameOrigin);
            *status.write().await = Some(outcome.clone());

            if changed {
                report_public_read(&outcome);
            }

            if same_origin {
                // Database storage with no CDN: this process serves the bytes,
                // so there is no third party whose permissions could drift and
                // nothing to re-check. Ending the task beats ticking forever to
                // re-derive a constant.
                return;
            }
            tokio::time::sleep(PUBLIC_READ_RECHECK).await;
        }
    });
}

fn report_public_read(outcome: &PublicReadProbe) {
    match outcome {
        PublicReadProbe::SameOrigin => {}
        PublicReadProbe::Readable => {
            tracing::info!("Uploads are publicly readable — image URLs will resolve");
        }
        PublicReadProbe::Forbidden => {
            tracing::warn!(
                    "UPLOADS ARE NOT PUBLICLY READABLE. An anonymous request to the upload \
                     origin was refused, which is exactly what every visitor's browser will \
                     get: avatars, logos and post images will all be broken links, and \
                     nothing on the server will report it. Grant anonymous read on the \
                     bucket — for Cloud Storage: `gcloud storage buckets add-iam-policy-binding \
                     gs://YOUR_BUCKET --member=allUsers \
                     --role=roles/storage.legacyObjectReader`. Use that role, NOT \
                     objectViewer: objectViewer also carries storage.objects.list, which would \
                     let anyone on the internet enumerate every object in the bucket. With \
                     uniform bucket-level access enabled (recommended) the IAM binding is the \
                 only mechanism; per-object ACLs are ignored."
            );
        }
        PublicReadProbe::Inconclusive(why) => {
            // Not a warning: the probe races the HTTP listener when a CDN is
            // pointed back at this app, and a transient failure here says
            // nothing about the configuration.
            tracing::info!("Could not verify public read access to uploads: {why}");
        }
    }
}

fn warn_degraded_capabilities(config: &Config) {
    if config.redis_url.is_none() {
        // Only the first clause was ever true. Jobs run through
        // `InlineJobRunner` (tokio::spawn) in both branches, and SSE is served
        // by the in-process `SseBroadcaster`, which never consulted Redis — the
        // old text sent operators looking for a Redis fault to explain
        // behaviour that was working as designed.
        tracing::warn!(
            "Redis disabled: rate limit and cache are per-instance — \
             correct for a single process, not for a horizontally scaled deployment"
        );
    }
    if config.s3_endpoint.is_none() && config.gcs_bucket.is_none() {
        // An HTTPS APP_URL is the same signal `cookies_secure` uses to decide a
        // deployment is real rather than a laptop.
        if config.app_url.starts_with("https://") {
            // Escalated from the generic notice below because this is the one
            // degraded capability *measured* to hurt under load: flooding
            // /files/ with 120 concurrent requests for a 1.95 MB image took page
            // rendering from ~20ms to ~800ms, and that number did not move when
            // the blob-read cap or the pool size were changed. The cost is
            // reading and shipping the bytes, and it competes with serving pages
            // because it shares the same process and database.
            //
            tracing::warn!(
                "APP_URL is https and uploads are served out of PostgreSQL by this process. \
                 A burst of image requests measurably slows page rendering and no in-app \
                 limit prevents it. Either put a caching reverse proxy in front of /files/ — \
                 responses already carry `Cache-Control: immutable` and an ETag, so a warm \
                 cache keeps this traffic off the origin entirely — or move the bytes off \
                 this process with S3_ENDPOINT (`--features s3`) or GCS_BUCKET \
                 (`--features gcs`)."
            );
        }
        tracing::warn!(
            "No object store: uploads stored in PostgreSQL — suitable for small-scale \
             deployments. Set S3_ENDPOINT or GCS_BUCKET to move them out"
        );
    }
    if !config.rate_limit_enabled {
        tracing::warn!("Rate limiting disabled — do NOT use in production");
    }
    // A production deployment terminates TLS at a proxy, so an https APP_URL with
    // no trusted-proxy hop is almost always a misconfiguration rather than a
    // choice. `extract_client_ip` ignores forwarded headers entirely at 0 and the
    // middleware falls back to the peer address — which behind nginx is nginx.
    // Every visitor then shares one counter, so the first few exhaust the budget
    // for everyone else. Worth an explicit warning because the symptom
    // (widespread 429s that vanish when rate limiting is turned off) points
    // nowhere near the cause.
    if config.rate_limit_enabled
        && config.trusted_proxy_count == 0
        && config.app_url.starts_with("https://")
    {
        tracing::warn!(
            "TRUSTED_PROXY_COUNT is 0 but APP_URL is https — if a proxy terminates TLS, \
             every client is seen as that proxy's IP and they all share one rate-limit \
             bucket. Set TRUSTED_PROXY_COUNT to the number of proxies in front of this \
             process (1 for a single nginx)."
        );
    }
}

/// Read the `color_scheme` field from `themes/{slug}/theme.json`.
/// Returns "auto" when the file is missing, unreadable, or has no `color_scheme` key.
pub fn read_theme_color_scheme(themes_dir: &str, slug: &str) -> String {
    let path = std::path::Path::new(themes_dir).join(slug).join("theme.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("color_scheme")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "auto".to_string())
}

/// Every installed theme's `locales/` directory, sorted for a deterministic
/// merge order.
///
/// Themes are catalog contributors on the same footing as core: a theme that
/// introduces its own copy ships the strings next to the templates that use
/// them, rather than requiring an edit to the core catalog it does not own.
/// Sorting matters because two themes could define the same key and directory
/// iteration order is not stable across filesystems.
fn theme_locale_roots(themes_dir: &str) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(themes_dir) else {
        return Vec::new();
    };
    let mut roots: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path().join("locales"))
        .filter(|p| p.is_dir())
        .collect();
    roots.sort();
    roots
}
