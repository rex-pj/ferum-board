use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ferum_domain::models::user::{TrustLevel, User, UserPreferences};
use ferum_domain::repositories::user_repository::{NewUser, UpdateUser, UserRepository};
use ferum_domain::AppError;

mockall::mock! {
    pub UserRepository {}

    #[async_trait]
    impl UserRepository for UserRepository {
        async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError>;
        async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError>;
        async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError>;
        async fn find_by_username(&self, username: &str) -> Result<Option<User>, AppError>;
        async fn create(&self, user: NewUser) -> Result<User, AppError>;
        async fn update(&self, id: Uuid, patch: UpdateUser) -> Result<User, AppError>;
        async fn increment_failed_login(&self, id: Uuid) -> Result<i32, AppError>;
        async fn reset_failed_login(&self, id: Uuid) -> Result<(), AppError>;
        async fn lock_until(&self, id: Uuid, until: DateTime<Utc>) -> Result<(), AppError>;
        async fn set_email_verified(&self, id: Uuid) -> Result<(), AppError>;
        async fn set_trust_level(&self, id: Uuid, level: TrustLevel) -> Result<(), AppError>;
        async fn set_password_hash(&self, id: Uuid, hash: String) -> Result<(), AppError>;
        async fn count_admins(&self) -> Result<u64, AppError>;
        async fn list_paginated<'a>(&self, page: u64, per_page: u64, search: Option<&'a str>, sort_by: Option<&'a str>, sort_dir: Option<&'a str>) -> Result<(Vec<User>, u64), AppError>;
        async fn get_preferences(&self, user_id: Uuid) -> Result<UserPreferences, AppError>;
        async fn upsert_preferences(&self, prefs: UserPreferences) -> Result<(), AppError>;
        async fn set_avatar(&self, user_id: Uuid, file_key: String) -> Result<(), AppError>;
        async fn remove_avatar(&self, user_id: Uuid) -> Result<(), AppError>;
        async fn set_cover(&self, user_id: Uuid, file_key: String) -> Result<(), AppError>;
        async fn remove_cover(&self, user_id: Uuid) -> Result<(), AppError>;
        async fn update_last_seen(&self, user_id: Uuid) -> Result<(), AppError>;
        async fn increment_post_count(&self, user_id: Uuid, delta: i32) -> Result<(), AppError>;
        async fn increment_trust_score(&self, user_id: Uuid, amount: i32) -> Result<(), AppError>;
        async fn get_watched_categories(&self, user_id: Uuid) -> Result<Vec<Uuid>, AppError>;
        async fn get_muted_categories(&self, user_id: Uuid) -> Result<Vec<Uuid>, AppError>;
        async fn watch_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError>;
        async fn unwatch_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError>;
        async fn mute_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError>;
        async fn unmute_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError>;
    }
}
