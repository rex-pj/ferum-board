use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::application::ports::{CacheService, NotificationBus, RateLimiter, StorageService, TokenService};
use crate::application::usecases::admin_stats_usecase::AdminStatsUseCase;
use crate::infrastructure::notification::SseBroadcaster;
use crate::application::usecases::admin_usecase::AdminUseCase;
use crate::application::usecases::auth_usecase::AuthUseCase;
use crate::application::usecases::bookmark_usecase::BookmarkUseCase;
use crate::application::usecases::webhook_usecase::WebhookUseCase;
use crate::application::usecases::category_usecase::CategoryUseCase;
use crate::application::usecases::moderation_usecase::ModerationUseCase;
use crate::application::usecases::notification_usecase::NotificationUseCase;
use crate::application::usecases::post_usecase::PostUseCase;
use crate::application::usecases::reaction_usecase::ReactionUseCase;
use crate::application::usecases::search_usecase::SearchUseCase;
use crate::application::usecases::thread_usecase::ThreadUseCase;
use crate::application::usecases::setup_usecase::SetupUseCase;
use crate::application::usecases::user_usecase::UserUseCase;
use crate::domain::repositories::{CategoryModeratorRepository, StoredFileRepository};
use crate::infrastructure::repositories::PgSiteConfigRepository;

#[allow(dead_code)]
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub setup: Arc<SetupUseCase>,
    pub auth: Arc<AuthUseCase>,
    pub admin: Arc<AdminUseCase>,
    pub admin_stats: Arc<AdminStatsUseCase>,
    pub bookmark: Arc<BookmarkUseCase>,
    pub category: Arc<CategoryUseCase>,
    pub thread: Arc<ThreadUseCase>,
    pub post: Arc<PostUseCase>,
    pub reaction: Arc<ReactionUseCase>,
    pub notification: Arc<NotificationUseCase>,
    pub moderation: Arc<ModerationUseCase>,
    pub search: Arc<SearchUseCase>,
    pub user: Arc<UserUseCase>,
    pub webhook: Arc<WebhookUseCase>,
    pub site_config: Arc<PgSiteConfigRepository>,
    pub cat_mod_repo: Arc<dyn CategoryModeratorRepository>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub token_service: Arc<dyn TokenService>,
    pub cache: Arc<dyn CacheService>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub storage: Arc<dyn StorageService>,
    pub notification_bus: Arc<dyn NotificationBus>,
    pub broadcaster: Arc<SseBroadcaster>,
    /// True when APP_URL starts with https:// — adds the Secure flag to auth cookies.
    pub cookies_secure: bool,
}
