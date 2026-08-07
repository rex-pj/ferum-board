use std::sync::Arc;

use bytes::Bytes;

use std::time::Duration;

use crate::constants::{MAX_AVATAR_BYTES, MAX_COVER_BYTES};
use crate::permission::PermissionChecker;
use crate::ports::{CacheService, ForumJob, JobQueue, PasswordHasher, StorageService};
use crate::shared::{AppError, OptionExt};
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::validate_image_magic;
use ferum_domain::models::user::{User, UserPreferences};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::user_repository::{UpdateUser, UserRepository};
use ferum_domain::AuthUser;

/// How long a cached preferences read may be stale after a write on another
/// request/instance. Kept short, not because correctness demands it (every
/// write already invalidates its own key) but to bound staleness from any
/// cache backend inconsistency.
const PREFERENCES_CACHE_TTL: Duration = Duration::from_secs(300);

/// Shared with CategoryUseCase's watch/mute methods, which mutate the same
/// underlying preferences row through a different repository call.
pub(crate) fn preferences_cache_key(user_id: uuid::Uuid) -> String {
    format!("user:prefs:{user_id}")
}

pub struct UserUseCase {
    pub users: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    /// Where blob bytes live, and the only authority on what a file's URL looks
    /// like. `stored_files` owns the metadata and reference count beside it.
    pub storage: Arc<dyn StorageService>,
    pub jobs: Arc<dyn JobQueue>,
    /// Optional — when unset, get_preferences just always hits the DB.
    /// Exists so every page render (via user_ctx()) can cheaply read the
    /// viewer's theme/font/layout preferences without a 3-way JOIN per request.
    pub cache: Option<Arc<dyn CacheService>>,
}

impl UserUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        hasher: Arc<dyn PasswordHasher>,
        stored_files: Arc<dyn StoredFileRepository>,
        storage: Arc<dyn StorageService>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self {
            users,
            hasher,
            stored_files,
            storage,
            jobs,
            cache: None,
        }
    }

    pub fn with_cache(mut self, cache: Arc<dyn CacheService>) -> Self {
        self.cache = Some(cache);
        self
    }

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id))]
    pub async fn update_profile(
        &self,
        actor: &AuthUser,
        cmd: UpdateProfileCmd,
    ) -> Result<User, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        if let Some(ref v) = cmd.display_name {
            if !crate::validators::validate_display_name(v) {
                return Err(AppError::invalid("display_name_length"));
            }
        }
        if let Some(ref v) = cmd.bio {
            if !crate::validators::validate_bio(v) {
                return Err(AppError::invalid("bio_too_long"));
            }
        }
        if let Some(ref v) = cmd.website {
            if !crate::validators::validate_website(v) {
                return Err(AppError::invalid("invalid_website_url"));
            }
        }

        self.users
            .update(
                actor.id,
                UpdateUser {
                    display_name: cmd.display_name.map(Some),
                    bio: cmd.bio.map(Some),
                    website: cmd.website.map(Some),
                    ..Default::default()
                },
            )
            .await
    }

    /// Returns the session epoch published by this change, when a cache is
    /// wired. The caller must mint any replacement access token with this value
    /// as its `iat` — see [`crate::usecases::invalidate_sessions`].
    #[tracing::instrument(skip_all, fields(user_id = %actor.id))]
    pub async fn change_password(
        &self,
        actor: &AuthUser,
        current_password: &str,
        new_password: &str,
    ) -> Result<Option<i64>, AppError> {
        if !crate::validators::validate_password(new_password) {
            return Err(AppError::invalid("password_requirements"));
        }

        let user = self
            .users
            .find_by_id(actor.id)
            .await?
            .or_not_found()?;
        let hash = user
            .password_hash
            .as_deref()
            .ok_or(AppError::forbidden("no_password_set"))?;

        if !self.hasher.verify(current_password, hash).await? {
            return Err(AppError::forbidden("incorrect_current_password"));
        }

        let new_hash = self.hasher.hash(new_password).await?;
        self.users.set_password_hash(actor.id, new_hash).await?;

        // Changing a password must end sessions opened with the old one —
        // otherwise a token stolen before the change keeps working until it
        // expires on its own. Only possible where a cache is wired; without one
        // there is nowhere to publish the epoch and behaviour is unchanged.
        //
        // Returns the epoch so the caller can mint the replacement token with a
        // matching `iat`; anything earlier would be revoked by the very epoch
        // this call just published, logging the user out of the session they
        // are currently using.
        let epoch = match &self.cache {
            Some(cache) => Some(crate::usecases::invalidate_sessions(cache.as_ref(), actor.id).await),
            None => None,
        };
        Ok(epoch)
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn get_preferences(&self, actor: &AuthUser) -> Result<UserPreferences, AppError> {
        self.get_preferences_by_id(actor.id).await
    }

    /// Cached read, keyed by user_id directly — used by `user_ctx()` on every
    /// page render (there's no `AuthUser` handy there beyond the id) as well
    /// as by `get_preferences` above.
    pub async fn get_preferences_by_id(&self, user_id: uuid::Uuid) -> Result<UserPreferences, AppError> {
        let Some(cache) = &self.cache else {
            return self.users.get_preferences(user_id).await;
        };

        let key = preferences_cache_key(user_id);
        if let Some(cached) = cache.get(&key).await {
            if let Ok(prefs) = serde_json::from_str::<UserPreferences>(&cached) {
                return Ok(prefs);
            }
        }

        let prefs = self.users.get_preferences(user_id).await?;
        if let Ok(json) = serde_json::to_string(&prefs) {
            let _ = cache.set(&key, &json, PREFERENCES_CACHE_TTL).await;
        }
        Ok(prefs)
    }

    #[tracing::instrument(skip(self, actor, prefs), fields(user_id = %actor.id))]
    pub async fn update_preferences(
        &self,
        actor: &AuthUser,
        prefs: UserPreferences,
    ) -> Result<(), AppError> {
        let mut p = prefs;
        p.user_id = actor.id;
        self.users.upsert_preferences(p).await?;
        if let Some(cache) = &self.cache {
            let _ = cache.del(&preferences_cache_key(actor.id)).await;
        }
        Ok(())
    }

    // ─── Avatar — CAS upload flow ─────────────────────────────────────────────

    #[tracing::instrument(skip(self, actor, data, content_type), fields(user_id = %actor.id))]
    pub async fn set_avatar(
        &self,
        actor: &AuthUser,
        data: Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::can_upload_profile_image(actor)?;

        if !validate_image_content_type(&content_type) || !validate_image_magic(&data) {
            return Err(AppError::invalid("avatar_invalid_type"));
        }
        if data.len() > MAX_AVATAR_BYTES {
            return Err(AppError::invalid_with("avatar_too_large", [("limit_mb", (MAX_AVATAR_BYTES / (1024 * 1024)).into())]));
        }

        let size = data.len() as i64;
        let key = cas_key("avatars", &data, &content_type);

        // Bytes first, then the row that makes the key discoverable. The order
        // matters: the row is what any reader resolves a URL through, so
        // creating it before the content exists would publish a link to nothing.
        self.storage.put(&key, data, &content_type).await?;
        self.stored_files
            .upsert_and_ref(&key, &content_type, size, Some(actor.id))
            .await?;

        // Swap pointer — get old key before overwriting
        let user = self
            .users
            .find_by_id(actor.id)
            .await?
            .or_not_found()?;
        let old_key = user
            .avatar_url
            .as_deref()
            .and_then(|url| self.storage.key_from_url(url));

        self.users.set_avatar(actor.id, key.clone()).await?;

        // Release old reference — GC if ref_count hits 0
        if let Some(old) = old_key.filter(|k| k != &key) {
            let remaining = self.stored_files.decrement_ref(&old).await?;
            if remaining == 0 {
                self.jobs
                    .enqueue(ForumJob::GcStorageKey { key: old })
                    .await?;
            }
        }

        // Same form the repository returns on read, so an avatar has ONE URL
        // whether you just uploaded it or reloaded the page. See `ports::file_url`.
        Ok(crate::ports::file_url(&key))
    }

    // ─── Cover — CAS upload flow ──────────────────────────────────────────────

    #[tracing::instrument(skip(self, actor, data, content_type), fields(user_id = %actor.id))]
    pub async fn set_cover(
        &self,
        actor: &AuthUser,
        data: Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::can_upload_profile_image(actor)?;

        if !validate_image_content_type(&content_type) || !validate_image_magic(&data) {
            return Err(AppError::invalid("cover_invalid_type"));
        }
        if data.len() > MAX_COVER_BYTES {
            return Err(AppError::invalid_with("cover_too_large", [("limit_mb", (MAX_COVER_BYTES / (1024 * 1024)).into())]));
        }

        let size = data.len() as i64;
        let key = cas_key("covers", &data, &content_type);

        self.storage.put(&key, data, &content_type).await?;
        self.stored_files
            .upsert_and_ref(&key, &content_type, size, Some(actor.id))
            .await?;

        let user = self
            .users
            .find_by_id(actor.id)
            .await?
            .or_not_found()?;
        let old_key = user
            .cover_url
            .as_deref()
            .and_then(|url| self.storage.key_from_url(url));

        self.users.set_cover(actor.id, key.clone()).await?;

        if let Some(old) = old_key.filter(|k| k != &key) {
            let remaining = self.stored_files.decrement_ref(&old).await?;
            if remaining == 0 {
                self.jobs
                    .enqueue(ForumJob::GcStorageKey { key: old })
                    .await?;
            }
        }

        // Same form the repository returns on read, so an avatar has ONE URL
        // whether you just uploaded it or reloaded the page. See `ports::file_url`.
        Ok(crate::ports::file_url(&key))
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn remove_cover(&self, actor: &AuthUser) -> Result<(), AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let user = self
            .users
            .find_by_id(actor.id)
            .await?
            .or_not_found()?;

        if let Some(url) = &user.cover_url {
            if let Some(key) = self.storage.key_from_url(url) {
                let key = key.as_str();
                let remaining = self.stored_files.decrement_ref(key).await?;
                if remaining == 0 {
                    self.jobs
                        .enqueue(ForumJob::GcStorageKey {
                            key: key.to_string(),
                        })
                        .await?;
                }
            }
        }

        self.users.remove_cover(actor.id).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn remove_avatar(&self, actor: &AuthUser) -> Result<(), AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let user = self
            .users
            .find_by_id(actor.id)
            .await?
            .or_not_found()?;

        if let Some(url) = &user.avatar_url {
            if let Some(key) = self.storage.key_from_url(url) {
                let key = key.as_str();
                let remaining = self.stored_files.decrement_ref(key).await?;
                if remaining == 0 {
                    self.jobs
                        .enqueue(ForumJob::GcStorageKey {
                            key: key.to_string(),
                        })
                        .await?;
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
