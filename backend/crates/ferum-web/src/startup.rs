use std::sync::Arc;

use crate::app_state::AppState;
use crate::config::Config;
use crate::handlers::admin::api::config::smtp_settings_from_config;
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
use ferum_domain::repositories::{SiteConfigRepository, ThemeRepository};
#[cfg(feature = "meilisearch")]
use ferum_infrastructure::search::MeilisearchService;
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
    storage::DatabaseStorageService,
    system_seed_service::PgSystemSeedService,
};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

/// Pool options shared by the write and read connections. Pool sizing comes
/// from DB_MAX_CONNECTIONS / DB_MIN_CONNECTIONS; timeouts are fixed.
fn build_connect_options(url: &str, config: &Config) -> ConnectOptions {
    let mut opts = ConnectOptions::new(url);
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

    // ─── PostgreSQL ─────────────────────────────────────────────────────────
    let pg_write = Database::connect(build_connect_options(&config.database_url, config)).await?;

    // ─── Run database migrations ─────────────────────────────────────────────
    tracing::info!("Running database migrations...");
    migration::Migrator::up(&pg_write, None).await?;
    tracing::info!("Migrations completed successfully");

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
    #[cfg(feature = "s3")]
    let storage: Arc<dyn StorageService> = match &config.s3_endpoint {
        Some(endpoint) => {
            tracing::info!("S3_ENDPOINT set — using S3 object storage");
            let access_key = config.s3_access_key.as_deref().unwrap_or_default();
            let secret_key = config.s3_secret_key.as_deref().unwrap_or_default();
            let bucket = config.s3_bucket.as_deref().unwrap_or("forum-uploads");
            let cdn_base = config.cdn_base_url.as_deref().unwrap_or(endpoint);
            Arc::new(
                S3StorageService::new(
                    endpoint,
                    access_key,
                    secret_key,
                    bucket,
                    &config.s3_region,
                    cdn_base,
                )
                .await,
            )
        }
        None => {
            tracing::info!("S3_ENDPOINT not set — using database storage");
            Arc::new(DatabaseStorageService::new(pg_write.clone()))
        }
    };
    #[cfg(not(feature = "s3"))]
    let storage: Arc<dyn StorageService> = {
        tracing::info!("S3 feature disabled — using database storage");
        Arc::new(DatabaseStorageService::new(pg_write.clone()))
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
    let (cache, rate_limiter_raw, job_queue, notification_bus): (
        Arc<dyn CacheService>,
        Arc<dyn RateLimiter>,
        Arc<dyn JobQueue>,
        Arc<dyn NotificationBus>,
    ) = match &config.redis_url {
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
        .with_plugin_runtime(plugin_hooks.clone()),
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
        .with_stored_files(stored_file_repo.clone()),
    );

    let reaction = Arc::new(
        ReactionUseCase::new(
            reaction_repo.clone(),
            post_repo.clone(),
            thread_repo.clone(),
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
        job_queue.clone(),
    ));
    let review = Arc::new(ReviewUseCase::new(review_rating_repo));

    let moderation = Arc::new(
        ModerationUseCase::new(
            report_repo,
            post_repo.clone(),
            thread_repo.clone(),
            user_repo.clone(),
            notification_repo,
            Arc::new(PgAuditLogRepository::new(pg_write.clone())),
            event_bus.clone(),
            cache.clone(),
        )
        .with_plugin_runtime(plugin_hooks.clone()),
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
            job_queue.clone(),
        )
        .with_cache(cache.clone()),
    );

    let bookmark = Arc::new(BookmarkUseCase::new(bookmark_repo, thread_repo.clone()));
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
    let plugin = Arc::new(PluginUseCase::new(
        plugin_repo,
        webhook_repo.clone(),
        plugin_lifecycle,
        plugin_db_gateway,
        stored_file_repo.clone(),
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

    let cookies_secure = config.app_url.starts_with("https://");
    warn_degraded_capabilities(config);

    Ok(AppState {
        db: pg_write.clone(),
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

fn warn_degraded_capabilities(config: &Config) {
    if config.redis_url.is_none() {
        tracing::warn!(
            "Redis disabled: rate limit is per-instance, jobs are synchronous, SSE unavailable"
        );
    }
    if config.s3_endpoint.is_none() {
        tracing::warn!(
            "S3 disabled: uploads stored in PostgreSQL — suitable for small-scale deployments"
        );
    }
    if !config.rate_limit_enabled {
        tracing::warn!("Rate limiting disabled — do NOT use in production");
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
