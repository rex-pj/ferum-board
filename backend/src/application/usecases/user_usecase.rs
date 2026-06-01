use std::sync::Arc;

use bytes::Bytes;

use crate::application::permission::PermissionChecker;
use crate::application::ports::{ForumJob, JobQueue, PasswordHasher};
use crate::application::shared::AppError;
use crate::constants::{MAX_AVATAR_BYTES};
use crate::domain::models::user::{User, UserPreferences};
use crate::domain::repositories::stored_file_repository::StoredFileRepository;
use crate::domain::repositories::user_repository::{UpdateUser, UserRepository};
use crate::infrastructure::storage::cas::{cas_key, validate_image_content_type};
use crate::middleware::auth::AuthUser;

pub struct UserUseCase {
    pub users: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub jobs: Arc<dyn JobQueue>,
}

impl UserUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        hasher: Arc<dyn PasswordHasher>,
        stored_files: Arc<dyn StoredFileRepository>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self { users, hasher, stored_files, jobs }
    }

    pub async fn update_profile(&self, actor: &AuthUser, cmd: UpdateProfileCmd) -> Result<User, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        if let Some(ref v) = cmd.display_name {
            if !crate::validators::validate_display_name(v) {
                return Err(AppError::unprocessable("Display name must be 1–60 characters"));
            }
        }
        if let Some(ref v) = cmd.bio {
            if !crate::validators::validate_bio(v) {
                return Err(AppError::unprocessable("Bio must be 500 characters or fewer"));
            }
        }
        if let Some(ref v) = cmd.website {
            if !crate::validators::validate_website(v) {
                return Err(AppError::unprocessable(
                    "Website must be an http:// or https:// URL, up to 255 characters",
                ));
            }
        }

        self.users.update(actor.id, UpdateUser {
            display_name: cmd.display_name.map(Some),
            bio: cmd.bio.map(Some),
            website: cmd.website.map(Some),
            ..Default::default()
        }).await
    }

    pub async fn change_password(
        &self,
        actor: &AuthUser,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AppError> {
        if !crate::validators::validate_password(new_password) {
            return Err(AppError::unprocessable(crate::validators::PASSWORD_REQUIREMENTS));
        }

        let user = self.users.find_by_id(actor.id).await?.ok_or(AppError::NotFound)?;
        let hash = user.password_hash.as_deref().ok_or(AppError::forbidden("no_password_set"))?;

        if !self.hasher.verify(current_password, hash).await? {
            return Err(AppError::forbidden("incorrect_current_password"));
        }

        let new_hash = self.hasher.hash(new_password).await?;
        self.users.set_password_hash(actor.id, new_hash).await
    }

    pub async fn get_preferences(&self, actor: &AuthUser) -> Result<UserPreferences, AppError> {
        self.users.get_preferences(actor.id).await
    }

    pub async fn update_preferences(&self, actor: &AuthUser, prefs: UserPreferences) -> Result<(), AppError> {
        let mut p = prefs;
        p.user_id = actor.id;
        self.users.upsert_preferences(p).await
    }

    // ─── Avatar — CAS upload flow ─────────────────────────────────────────────

    pub async fn set_avatar(
        &self,
        actor: &AuthUser,
        data: Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::can_upload(actor)?;

        if !validate_image_content_type(&content_type) {
            return Err(AppError::unprocessable("avatar must be jpeg, png, webp, or gif"));
        }
        if data.len() > MAX_AVATAR_BYTES {
            return Err(AppError::unprocessable("avatar exceeds 5 MB size limit"));
        }

        let key = cas_key("avatars", &data, &content_type);

        // CAS: insert if not exists, otherwise just increment ref_count
        if self.stored_files.exists(&key).await? {
            self.stored_files.increment_ref(&key).await?;
        } else {
            self.stored_files
                .upsert(&key, &content_type, &data, data.len() as i64, Some(actor.id))
                .await?;
        }

        // Swap pointer — get old key before overwriting
        let user = self.users.find_by_id(actor.id).await?.ok_or(AppError::NotFound)?;
        let old_key = user.avatar_url
            .as_deref()
            .and_then(|url| url.strip_prefix("/files/"))
            .map(|k| k.to_string());

        self.users.set_avatar(actor.id, key.clone()).await?;

        // Release old reference — GC if ref_count hits 0
        if let Some(old) = old_key.filter(|k| k != &key) {
            let remaining = self.stored_files.decrement_ref(&old).await?;
            if remaining == 0 {
                self.jobs.enqueue(ForumJob::GcStorageKey { key: old }).await?;
            }
        }

        Ok(format!("/files/{key}"))
    }

    pub async fn remove_avatar(&self, actor: &AuthUser) -> Result<(), AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let user = self.users.find_by_id(actor.id).await?.ok_or(AppError::NotFound)?;

        if let Some(url) = &user.avatar_url {
            if let Some(key) = url.strip_prefix("/files/") {
                let remaining = self.stored_files.decrement_ref(key).await?;
                if remaining == 0 {
                    self.jobs.enqueue(ForumJob::GcStorageKey { key: key.to_string() }).await?;
                }
            }
        }

        self.users.remove_avatar(actor.id).await
    }
}

#[derive(Debug, Default)]
pub struct UpdateProfileCmd {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
}
