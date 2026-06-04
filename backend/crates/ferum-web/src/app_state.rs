use std::sync::Arc;

use sea_orm::DatabaseConnection;

use ferum_application::ports::{
    CacheService, NotificationBus, RateLimiter, StorageService, TokenService,
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
use ferum_application::usecases::role_usecase::RoleUseCase;
use ferum_application::usecases::search_usecase::SearchUseCase;
use ferum_application::usecases::setup_usecase::SetupUseCase;
use ferum_application::usecases::tag_usecase::TagUseCase;
use ferum_application::usecases::thread_usecase::ThreadUseCase;
use ferum_application::usecases::user_usecase::UserUseCase;
use ferum_application::usecases::webhook_usecase::WebhookUseCase;
use ferum_domain::repositories::{SiteConfigRepository, StoredFileRepository, UserRoleRepository};
use ferum_infrastructure::notification::SseBroadcaster;
use ferum_infrastructure::role_permission_cache::RolePermissionCache;

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
    pub role: Arc<RoleUseCase>,
    pub tag: Arc<TagUseCase>,
    pub webhook: Arc<WebhookUseCase>,
    pub site_config: Arc<dyn SiteConfigRepository>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub user_role_repo: Arc<dyn UserRoleRepository>,
    pub role_permission_cache: Arc<RolePermissionCache>,
    pub token_service: Arc<dyn TokenService>,
    pub cache: Arc<dyn CacheService>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub storage: Arc<dyn StorageService>,
    pub notification_bus: Arc<dyn NotificationBus>,
    pub broadcaster: Arc<SseBroadcaster>,
    /// True when APP_URL starts with https:// — adds the Secure flag to auth cookies.
    pub cookies_secure: bool,
}
