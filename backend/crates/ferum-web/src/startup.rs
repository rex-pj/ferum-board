use std::sync::Arc;

use crate::app_state::AppState;
use crate::config::Config;
use ferum_application::event_bus::EventBus;
use ferum_application::ports::{
    CacheService, JobQueue, NotificationBus, RateLimiter, SearchService, StorageService,
};
use ferum_application::usecases::admin_stats_usecase::AdminStatsUseCase;
use ferum_application::usecases::admin_usecase::AdminUseCase;
use ferum_application::usecases::auth_usecase::AuthUseCase;
use ferum_application::usecases::bookmark_usecase::BookmarkUseCase;
use ferum_application::usecases::category_usecase::CategoryUseCase;
use ferum_application::usecases::moderation_usecase::ModerationUseCase;
use ferum_application::usecases::notification_usecase::NotificationUseCase;
use ferum_application::usecases::post_usecase::PostUseCase;
use ferum_application::usecases::reaction_usecase::ReactionUseCase;
use ferum_application::usecases::search_usecase::SearchUseCase;
use ferum_application::usecases::setup_usecase::SetupUseCase;
use ferum_application::usecases::thread_usecase::ThreadUseCase;
use ferum_application::usecases::user_usecase::UserUseCase;
use ferum_application::usecases::webhook_usecase::WebhookUseCase;
use ferum_domain::repositories::SiteConfigRepository;
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
    repositories::{
        PgAuditLogRepository, PgBookmarkRepository, PgCategoryModeratorRepository,
        PgCategoryRepository, PgNotificationRepository, PgPostRepository, PgReactionRepository,
        PgReportRepository, PgSiteConfigRepository, PgStoredFileRepository, PgThreadRepository,
        PgUserRepository, PgWebhookRepository,
    },
    search::PostgresFtsService,
    storage::DatabaseStorageService,
};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub async fn build_app_state(config: &Config) -> anyhow::Result<AppState> {
    // ─── PostgreSQL ─────────────────────────────────────────────────────────
    let mut write_opts = ConnectOptions::new(&config.database_url);
    write_opts
        .max_connections(50)
        .min_connections(5)
        .connect_timeout(std::time::Duration::from_secs(5))
        .acquire_timeout(std::time::Duration::from_secs(5));
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
                .max_connections(50)
                .min_connections(5)
                .connect_timeout(std::time::Duration::from_secs(5))
                .acquire_timeout(std::time::Duration::from_secs(5));
            Database::connect(read_opts).await?
        }
        None => {
            tracing::info!("No DATABASE_READ_URL — read queries use primary");
            pg_write.clone()
        }
    };

    // ─── Email service ──────────────────────────────────────────────────────
    let email = Arc::new(LettreEmailService::new(
        &config.smtp_host,
        config.smtp_port,
        config.smtp_user.as_deref(),
        config.smtp_pass.as_deref(),
        &config.from_email,
    )?);

    // ─── SSE broadcaster ─────────────────────────────────────────────────────
    let broadcaster = Arc::new(SseBroadcaster::new());
    let sse_bus: Arc<dyn NotificationBus> = Arc::new(SseNotificationBus::new(broadcaster.clone()));

    // ─── Storage (declared early — needed by JobExecutor) ────────────────────
    #[cfg(feature = "s3")]
    let storage: Arc<dyn StorageService> = match &config.s3_endpoint {
        Some(endpoint) => {
            tracing::info!("S3_ENDPOINT set — using S3 object storage");
            let access_key = config.s3_access_key.as_deref().unwrap_or_default();
            let secret_key = config.s3_secret_key.as_deref().unwrap_or_default();
            let bucket = config.s3_bucket.as_deref().unwrap_or("ferum-board");
            let cdn_base = config.cdn_base_url.as_deref().unwrap_or(endpoint);
            Arc::new(
                S3StorageService::new(
                    endpoint,
                    access_key,
                    secret_key,
                    bucket,
                    "us-east-1",
                    cdn_base,
                )
                .await,
            )
        }
        None => {
            tracing::info!(
                "S3_ENDPOINT not set — using database storage (files stored in PostgreSQL)"
            );
            Arc::new(DatabaseStorageService::new(pg_write.clone()))
        }
    };
    #[cfg(not(feature = "s3"))]
    let storage: Arc<dyn StorageService> = {
        tracing::info!("S3 feature disabled — using database storage (files stored in PostgreSQL)");
        Arc::new(DatabaseStorageService::new(pg_write.clone()))
    };

    // ─── Repositories (declared early — needed by JobExecutor) ───────────────
    let user_repo = Arc::new(PgUserRepository::new(pg_write.clone()));
    let category_repo = Arc::new(PgCategoryRepository::new(pg_write.clone()));
    let cat_mod_repo: Arc<dyn ferum_domain::repositories::CategoryModeratorRepository> =
        Arc::new(PgCategoryModeratorRepository::new(pg_write.clone()));
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

    // ─── Search ──────────────────────────────────────────────────────────────
    #[cfg(feature = "meilisearch")]
    let search_svc: Arc<dyn SearchService> = match &config.meilisearch_url {
        Some(url) => {
            tracing::info!("MEILISEARCH_URL set — using Meilisearch");
            Arc::new(MeilisearchService::new(
                url,
                config.meilisearch_key.as_deref(),
                "threads",
            ))
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
                    tracing::warn!(
                        "Redis cache connect failed ({}), falling back to in-memory",
                        e
                    );
                    Arc::new(InMemoryCacheService::new())
                });
            let rate_limiter = RedisRateLimiter::new(url)
                .await
                .map(|s| -> Arc<dyn RateLimiter> { Arc::new(s) })
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "Redis rate limiter connect failed ({}), falling back to in-memory",
                        e
                    );
                    Arc::new(InMemoryRateLimiter::new())
                });
            (
                cache,
                rate_limiter,
                Arc::new(InlineJobRunner::new(executor)),
                sse_bus,
            )
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

    // ─── Rate limit toggle ───────────────────────────────────────────────────
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
    let event_bus = Arc::new(EventBus::new(
        audit_log_repo,
        notification_repo.clone(),
        notification_bus.clone(),
        webhook_repo.clone(),
        job_queue.clone(),
    ));

    // ─── Use cases ───────────────────────────────────────────────────────────
    let auth = Arc::new(AuthUseCase::new(
        user_repo.clone(),
        hasher,
        token_service.clone(),
        cache.clone(),
        job_queue.clone(),
    ));

    let admin = Arc::new(AdminUseCase::new(
        category_repo.clone(),
        Arc::new(PgCategoryModeratorRepository::new(pg_write.clone())),
        user_repo.clone(),
        Arc::new(PgAuditLogRepository::new(pg_write.clone())),
        cache.clone(),
    ));

    let category = Arc::new(CategoryUseCase::new(
        category_repo.clone(),
        thread_repo.clone(),
    ));

    let thread = Arc::new(ThreadUseCase::new(
        thread_repo.clone(),
        category_repo.clone(),
        post_repo.clone(),
        job_queue.clone(),
        stored_file_repo.clone(),
        event_bus.clone(),
        cache.clone(),
    ));

    let post = Arc::new(PostUseCase::new(
        post_repo.clone(),
        thread_repo.clone(),
        category_repo.clone(),
        user_repo.clone(),
        reaction_repo.clone(),
        event_bus.clone(),
    ));

    let reaction = Arc::new(ReactionUseCase::new(
        reaction_repo.clone(),
        post_repo.clone(),
        thread_repo.clone(),
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
        event_bus,
        cache.clone(),
    ));

    let search = Arc::new(SearchUseCase::new(search_svc));

    let pg_write_arc = Arc::new(pg_write.clone());
    let admin_stats = Arc::new(AdminStatsUseCase::new(pg_write_arc));

    let hasher2 = Arc::new(BcryptPasswordHasher);
    let user = Arc::new(UserUseCase::new(
        user_repo.clone(),
        hasher2,
        stored_file_repo.clone(),
        job_queue.clone(),
    ));

    let bookmark = Arc::new(BookmarkUseCase::new(bookmark_repo, thread_repo.clone()));
    let webhook = Arc::new(WebhookUseCase::new(webhook_repo.clone()));

    let site_config: Arc<dyn SiteConfigRepository> =
        Arc::new(PgSiteConfigRepository::new(pg_write.clone()));

    let hasher3 = Arc::new(BcryptPasswordHasher);
    let bulk_seed = Arc::new(PgBulkSeedService::new(pg_write.clone()));
    let setup = Arc::new(SetupUseCase::new(
        user_repo,
        hasher3,
        token_service.clone(),
        cache.clone(),
        site_config.clone(),
        bulk_seed,
    ));

    let cookies_secure = config.app_url.starts_with("https://");
    warn_degraded_capabilities(config);

    Ok(AppState {
        db: pg_write.clone(),
        setup,
        auth,
        admin,
        admin_stats,
        bookmark,
        category,
        thread,
        post,
        reaction,
        notification,
        moderation,
        search,
        user,
        webhook,
        site_config,
        cat_mod_repo,
        stored_files: stored_file_repo,
        token_service,
        cache,
        rate_limiter,
        storage,
        notification_bus,
        broadcaster,
        cookies_secure,
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

    tracing::info!(
        "Headless setup complete — admin account created for '{}'",
        username
    );
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
