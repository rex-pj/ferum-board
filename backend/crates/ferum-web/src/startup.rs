use std::sync::Arc;

use crate::app_state::AppState;
use crate::config::Config;
use crate::tera_engine::TeraEngine;
use ferum_application::event_bus::{EventBus, EventPublisher};
use ferum_application::ports::{
    CacheService, JobQueue, NotificationBus, PermissionResolver, PluginHookRuntime,
    PluginLifecycle, PluginUiRuntime, RateLimiter, SearchService, StorageService,
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
use ferum_infrastructure::repositories::PgThemeRepository;
use ferum_domain::repositories::{SiteConfigRepository, ThemeRepository};
#[cfg(feature = "meilisearch")]
use ferum_infrastructure::search::MeilisearchService;
#[cfg(feature = "s3")]
use ferum_infrastructure::storage::S3StorageService;
use ferum_infrastructure::{
    bcrypt_password_hasher::BcryptPasswordHasher,
    bulk_seed_service::PgBulkSeedService,
    cache::{InMemoryCacheService, RedisCacheService},
    email::LettreEmailService,
    job_queue::{InlineJobRunner, JobExecutor},
    jwt_token_service::JwtTokenService,
    notification::{SseBroadcaster, SseNotificationBus},
    rate_limit::{InMemoryRateLimiter, NullRateLimiter, RedisRateLimiter},
    plugins::registry::PluginRegistry,
    repositories::{
        PgAuditLogRepository, PgBookmarkRepository, PgCategoryRepository, PgFollowRepository,
        PgNotificationRepository, PgPermissionRepository, PgPluginRepository, PgPostRepository,
        PgReactionRepository, PgReportRepository, PgRoleRepository, PgSiteConfigRepository,
        PgStatsRepository, PgStoredFileRepository, PgTagRepository, PgThreadRepository,
        PgUserRepository,
        PgUserRoleRepository, PgWebhookRepository,
    },
    role_permission_cache::RolePermissionCache,
    search::PostgresFtsService,
    storage::DatabaseStorageService,
};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub async fn build_app_state(config: &Config) -> anyhow::Result<AppState> {
    // ─── PostgreSQL ─────────────────────────────────────────────────────────
    let mut write_opts = ConnectOptions::new(&config.database_url);
    write_opts
        .max_connections(20)
        .min_connections(2)
        .connect_timeout(std::time::Duration::from_secs(5))
        .acquire_timeout(std::time::Duration::from_secs(5))
        .idle_timeout(std::time::Duration::from_secs(300))
        .max_lifetime(std::time::Duration::from_secs(1800))
        // SeaORM logs every statement through `tracing` by default; disable to remove
        // per-query formatting overhead on the hot path. Re-enable with a level filter
        // only when debugging queries.
        .sqlx_logging(false);
    let pg_write = Database::connect(write_opts).await?;

    // ─── Run database migrations ─────────────────────────────────────────────
    tracing::info!("Running database migrations...");
    migration::Migrator::up(&pg_write, None).await?;
    tracing::info!("Migrations completed successfully");

    let pg_read: DatabaseConnection = match &config.database_read_url {
        Some(url) => {
            tracing::info!("Read replica enabled");
            let mut read_opts = ConnectOptions::new(url);
            read_opts
                .max_connections(20)
                .min_connections(2)
                .connect_timeout(std::time::Duration::from_secs(5))
                .acquire_timeout(std::time::Duration::from_secs(5))
                .idle_timeout(std::time::Duration::from_secs(300))
                .max_lifetime(std::time::Duration::from_secs(1800))
                .sqlx_logging(false);
            Database::connect(read_opts).await?
        }
        None => {
            tracing::info!("No DATABASE_READ_URL — read queries use primary");
            pg_write.clone()
        }
    };

    // ─── Email service ──────────────────────────────────────────────────────
    // When SMTP_HOST is absent, email is disabled and users are auto-verified on registration.
    let smtp_enabled = config.smtp_host.is_some();
    let email = Arc::new(LettreEmailService::new(
        config.smtp_host.as_deref().unwrap_or("localhost"),
        config.smtp_port,
        config.smtp_user.as_deref(),
        config.smtp_pass.as_deref(),
        &config.from_email,
    )?);
    if !smtp_enabled {
        tracing::warn!(
            "SMTP_HOST not set — email sending disabled, new registrations are auto-verified"
        );
    }

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
            let bucket = config.s3_bucket.as_deref().unwrap_or("ferum-board");
            let cdn_base = config.cdn_base_url.as_deref().unwrap_or(endpoint);
            Arc::new(
                S3StorageService::new(endpoint, access_key, secret_key, bucket, "us-east-1", cdn_base)
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
            Arc::new(MeilisearchService::new(url, config.meilisearch_key.as_deref(), "threads"))
        }
        None => Arc::new(PostgresFtsService::new(pg_read.clone())),
    };
    #[cfg(not(feature = "meilisearch"))]
    let search_svc: Arc<dyn SearchService> = Arc::new(PostgresFtsService::new(pg_read.clone()));

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
            ));
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
            ));
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
        Arc::new(JwtTokenService::new(&config.jwt_secret));

    // ─── Plugin system (before event_bus so plugin_runtime can be injected) ──
    let plugin_repo = Arc::new(PgPluginRepository::new(pg_write.clone()));
    let plugin_registry = Arc::new(PluginRegistry::new(
        plugin_repo.clone(),
        cache.clone(),
        config.plugin_hook_timeout_ms,
        config.plugin_circuit_threshold,
    ));
    if let Err(e) = plugin_registry.load_from_db().await {
        tracing::warn!("Plugin registry failed to load from DB: {:?}", e);
    }
    // ISP: cast once to each focused trait so each consumer receives only the interface it needs.
    let plugin_hooks: Arc<dyn PluginHookRuntime> = plugin_registry.clone();
    let plugin_ui: Arc<dyn PluginUiRuntime> = plugin_registry.clone();
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
    if config.smtp_host.is_some() {
        let existing_smtp = site_config.get("smtp_host").await.unwrap_or(None);
        if existing_smtp.is_none() || existing_smtp.as_deref() == Some("") {
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
        .with_auto_verify_email(!smtp_enabled)
        .with_site_config(site_config.clone())
        .with_plugin_runtime(plugin_hooks.clone()),
    );

    let admin = Arc::new(AdminUseCase::new(
        category_repo.clone(),
        role_repo.clone(),
        user_role_repo.clone(),
        user_repo.clone(),
        Arc::new(PgAuditLogRepository::new(pg_write.clone())),
        cache.clone(),
    ));

    let role = Arc::new(RoleUseCase::new(
        role_repo.clone(),
        permission_repo.clone(),
        user_role_repo.clone(),
    ));

    let category = Arc::new(
        CategoryUseCase::new(category_repo.clone(), thread_repo.clone(), tag_repo.clone(), user_repo.clone())
            .with_site_config(site_config.clone()),
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
        .with_plugin_runtime(plugin_hooks.clone()),
    );

    let reaction = Arc::new(ReactionUseCase::new(
        reaction_repo.clone(),
        post_repo.clone(),
        thread_repo.clone(),
        user_repo.clone(),
        event_bus.clone(),
    ));

    let notification = Arc::new(NotificationUseCase::new(notification_repo.clone()));

    let moderation = Arc::new(ModerationUseCase::new(
        report_repo,
        post_repo.clone(),
        thread_repo.clone(),
        user_repo.clone(),
        notification_repo,
        Arc::new(PgAuditLogRepository::new(pg_write.clone())),
        event_bus.clone(),
        cache.clone(),
    ));

    let search = Arc::new(SearchUseCase::new(search_svc));

    let stats_repo = Arc::new(PgStatsRepository::new(pg_write.clone()));
    let admin_stats = Arc::new(AdminStatsUseCase::new(stats_repo).with_cache(cache.clone()));

    let hasher2 = Arc::new(BcryptPasswordHasher);
    let user = Arc::new(UserUseCase::new(
        user_repo.clone(),
        hasher2,
        stored_file_repo.clone(),
        job_queue.clone(),
    ));

    let bookmark = Arc::new(BookmarkUseCase::new(bookmark_repo, thread_repo.clone()));
    let follow_repo = Arc::new(PgFollowRepository::new(pg_write.clone()));
    let follow = Arc::new(FollowUseCase::new(
        follow_repo,
        user_repo.clone(),
        event_bus.clone(),
    ));
    let tag = Arc::new(TagUseCase::new(tag_repo.clone()));
    let webhook = Arc::new(WebhookUseCase::new(webhook_repo.clone()));
    let plugin = Arc::new(PluginUseCase::new(
        plugin_repo,
        webhook_repo.clone(),
        plugin_lifecycle,
        std::path::PathBuf::from(&config.plugins_dir),
    ));

    let hasher3 = Arc::new(BcryptPasswordHasher);
    let bulk_seed = Arc::new(PgBulkSeedService::new(pg_write.clone()));
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

    let tera = TeraEngine::new(
        std::path::PathBuf::from(&config.themes_dir),
        std::path::PathBuf::from(&config.admin_templates_dir),
        std::path::PathBuf::from(&config.static_dir),
    )
    .unwrap_or_else(|e| {
        tracing::warn!("TeraEngine init failed ({}), templates unavailable", e);
        TeraEngine::new(
            std::path::PathBuf::from("./frontend/themes"),
            std::path::PathBuf::from("./frontend/templates"),
            std::path::PathBuf::from("./frontend/static"),
        )
        .expect("TeraEngine fallback init failed")
    });

    // ─── Background: flush daily stats mỗi 5 phút ──────────────────────────────
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

    // ─── Background: flush view count buffer mỗi 60s ───────────────────────────
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
        site_config,
        site_config_cache,
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
        static_dir: config.static_dir.clone(),
        cookies_secure,
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
