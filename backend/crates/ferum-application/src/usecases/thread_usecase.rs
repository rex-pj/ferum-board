use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::constants::{MAX_THREADS_PER_PAGE, MAX_THUMBNAIL_BYTES, POST_EDIT_WINDOW_HOURS};
use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::ports::{CacheService, ForumJob, JobQueue};
use crate::shared::AppError;
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::generate_thread_slug;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::thread::{Thread, ThreadStatus};
use ferum_domain::models::user::UserRole;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository, UpdateThread};
use ferum_domain::AuthUser;

#[allow(dead_code)]
pub struct ThreadUseCase {
    pub threads: Arc<dyn ThreadRepository>,
    pub categories: Arc<dyn CategoryRepository>,
    pub posts: Arc<dyn PostRepository>,
    pub jobs: Arc<dyn JobQueue>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub event_bus: Arc<EventBus>,
    pub cache: Arc<dyn CacheService>,
}

impl ThreadUseCase {
    pub fn new(
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        posts: Arc<dyn PostRepository>,
        jobs: Arc<dyn JobQueue>,
        stored_files: Arc<dyn StoredFileRepository>,
        event_bus: Arc<EventBus>,
        cache: Arc<dyn CacheService>,
    ) -> Self {
        Self {
            threads,
            categories,
            posts,
            jobs,
            stored_files,
            event_bus,
            cache,
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    async fn find_live_thread(&self, id: Uuid) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        if thread.deleted_at.is_some() {
            return Err(AppError::NotFound);
        }
        Ok(thread)
    }

    fn require_author_or_mod(actor: &AuthUser, thread: &Thread) -> Result<(), AppError> {
        if thread.author_id != actor.id && actor.role < UserRole::Moderator {
            return Err(AppError::forbidden("not_author"));
        }
        Ok(())
    }

    async fn visible_category_map(
        &self,
        actor: Option<&AuthUser>,
    ) -> Result<HashMap<Uuid, (String, String)>, AppError> {
        const CACHE_KEY: &str = "forum:categories:all";

        let all_categories = if let Some(cached) = self.cache.get(CACHE_KEY).await {
            match serde_json::from_str(&cached) {
                Ok(cats) => cats,
                Err(_) => {
                    // Corrupt cache entry — fetch from DB and overwrite.
                    let cats = self.categories.list_all().await?;
                    if let Ok(json) = serde_json::to_string(&cats) {
                        self.cache
                            .set(CACHE_KEY, &json, Duration::from_secs(60))
                            .await
                            .ok();
                    }
                    cats
                }
            }
        } else {
            let cats = self.categories.list_all().await?;
            if let Ok(json) = serde_json::to_string(&cats) {
                self.cache
                    .set(CACHE_KEY, &json, Duration::from_secs(60))
                    .await
                    .ok();
            }
            cats
        };

        Ok(all_categories
            .iter()
            .filter(|c| PermissionChecker::can_view_category(actor, c).is_ok())
            .map(|c| (c.id, (c.slug.clone(), c.name.clone())))
            .collect())
    }

    fn enrich_threads(threads: &mut [Thread], map: &HashMap<Uuid, (String, String)>) {
        for thread in threads {
            if let Some((slug, name)) = map.get(&thread.category_id) {
                thread.category_slug = slug.clone();
                thread.category_name = Some(name.clone());
            }
        }
    }

    // ── Public API ───────────────────────────────────────────────────────────

    pub async fn list_by_category(
        &self,
        actor: Option<&AuthUser>,
        category_slug: &str,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category = self
            .categories
            .find_by_slug(category_slug)
            .await?
            .ok_or(AppError::NotFound)?;

        PermissionChecker::can_view_category(actor, &category)?;

        let per_page = per_page.min(MAX_THREADS_PER_PAGE);
        let (threads, total) = self
            .threads
            .list_by_category(category.id, page, per_page)
            .await?;
        let threads = threads
            .into_iter()
            .map(|mut t| {
                t.category_slug = category.slug.clone();
                t.category_name = Some(category.name.clone());
                t
            })
            .collect();
        Ok((threads, total))
    }

    pub async fn list_feed(
        &self,
        actor: Option<&AuthUser>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category_map = self.visible_category_map(actor).await?;
        let visible_ids: Vec<Uuid> = category_map.keys().cloned().collect();
        let per_page = per_page.min(MAX_THREADS_PER_PAGE);
        let (mut threads, total) = self.threads.list_feed(&visible_ids, page, per_page).await?;
        Self::enrich_threads(&mut threads, &category_map);
        Ok((threads, total))
    }

    pub async fn list_by_author(
        &self,
        actor: Option<&AuthUser>,
        author_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category_map = self.visible_category_map(actor).await?;
        let visible_ids: Vec<Uuid> = category_map.keys().cloned().collect();
        let per_page = per_page.min(MAX_THREADS_PER_PAGE);
        let (mut threads, total) = self
            .threads
            .list_by_author(author_id, &visible_ids, page, per_page)
            .await?;
        Self::enrich_threads(&mut threads, &category_map);
        Ok((threads, total))
    }

    pub async fn get_by_slug(
        &self,
        actor: Option<&AuthUser>,
        slug: &str,
    ) -> Result<Thread, AppError> {
        let mut thread = self
            .threads
            .find_by_slug(slug)
            .await?
            .ok_or(AppError::NotFound)?;

        if thread.status == ThreadStatus::Deleted {
            return Err(AppError::NotFound);
        }

        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .ok_or(AppError::NotFound)?;

        PermissionChecker::can_view_category(actor, &category)?;

        // Fire-and-forget: view count does not block the response.
        let threads_clone = self.threads.clone();
        let thread_id = thread.id;
        tokio::spawn(async move {
            if let Err(e) = threads_clone.increment_view_count(thread_id).await {
                tracing::warn!("view count increment failed for {}: {:?}", thread_id, e);
            }
        });

        thread.category_slug = category.slug;
        Ok(thread)
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        if thread.status == ThreadStatus::Deleted {
            return Err(AppError::NotFound);
        }
        Ok(thread)
    }

    pub async fn create(&self, actor: &AuthUser, cmd: CreateThreadCmd) -> Result<Thread, AppError> {
        let category = self
            .categories
            .find_by_id(cmd.category_id)
            .await?
            .ok_or(AppError::NotFound)?;

        PermissionChecker::can_create_post(actor, &category)?;

        // Pre-generate UUID so slug can embed it — collision-free, no retry needed
        let id = uuid::Uuid::new_v4();
        let slug = generate_thread_slug(&cmd.title, &id);

        self.threads
            .create(NewThread {
                id,
                category_id: cmd.category_id,
                author_id: actor.id,
                title: cmd.title,
                slug,
            })
            .await
    }

    pub async fn update_title(
        &self,
        actor: &AuthUser,
        id: Uuid,
        title: String,
    ) -> Result<Thread, AppError> {
        let thread = self.find_live_thread(id).await?;

        let is_author_within_window = thread.author_id == actor.id
            && Utc::now() - thread.created_at <= chrono::Duration::hours(POST_EDIT_WINDOW_HOURS);

        if !is_author_within_window && actor.role < UserRole::Moderator {
            return Err(AppError::forbidden("edit_window_expired"));
        }

        PermissionChecker::require_not_banned(actor)?;

        self.threads
            .update(
                id,
                UpdateThread {
                    title: Some(title),
                    ..Default::default()
                },
            )
            .await
    }

    pub async fn soft_delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let thread = self.find_live_thread(id).await?;
        Self::require_author_or_mod(actor, &thread)?;
        PermissionChecker::require_not_banned(actor)?;

        self.threads
            .update(
                id,
                UpdateThread {
                    status: Some(ThreadStatus::Deleted),
                    deleted_by_id: Some(actor.id),
                    deleted_at: Some(Utc::now()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    pub async fn pin(
        &self,
        actor: &AuthUser,
        id: Uuid,
        assigned: &[Uuid],
        pin: bool,
    ) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_moderate(actor, thread.category_id, assigned)?;
        self.threads
            .update(
                id,
                UpdateThread {
                    is_pinned: Some(pin),
                    ..Default::default()
                },
            )
            .await
    }

    pub async fn lock(
        &self,
        actor: &AuthUser,
        id: Uuid,
        assigned: &[Uuid],
        lock: bool,
    ) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_moderate(actor, thread.category_id, assigned)?;
        let status = if lock {
            ThreadStatus::Locked
        } else {
            ThreadStatus::Open
        };
        self.threads
            .update(
                id,
                UpdateThread {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await
    }

    pub async fn move_to(
        &self,
        actor: &AuthUser,
        id: Uuid,
        assigned: &[Uuid],
        target_category_id: Uuid,
    ) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_moderate(actor, thread.category_id, assigned)?;
        self.categories
            .find_by_id(target_category_id)
            .await?
            .ok_or(AppError::NotFound)?;
        self.threads
            .update(
                id,
                UpdateThread {
                    category_id: Some(target_category_id),
                    ..Default::default()
                },
            )
            .await
    }

    pub async fn mark_solved(
        &self,
        actor: &AuthUser,
        id: Uuid,
        best_answer_id: Uuid,
        assigned: &[Uuid],
    ) -> Result<Thread, AppError> {
        let thread = self.find_live_thread(id).await?;

        if thread.author_id != actor.id {
            PermissionChecker::can_moderate(actor, thread.category_id, assigned)?;
        }
        PermissionChecker::require_not_banned(actor)?;

        let best_post = self
            .posts
            .find_by_id(best_answer_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if best_post.thread_id != id {
            return Err(AppError::unprocessable(
                "best_answer must belong to this thread",
            ));
        }

        let result = self
            .threads
            .update(
                id,
                UpdateThread {
                    is_solved: Some(true),
                    best_answer_id: Some(Some(best_answer_id)),
                    ..Default::default()
                },
            )
            .await?;

        self.event_bus
            .publish(ForumEvent::BestAnswerMarked {
                post_id: best_answer_id,
                thread_id: id,
                thread_slug: thread.slug,
                post_author_id: best_post.author_id,
                by_user_id: actor.id,
            })
            .await;

        Ok(result)
    }

    pub async fn set_thumbnail(
        &self,
        actor: &AuthUser,
        thread_id: Uuid,
        data: bytes::Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::can_upload(actor)?;

        if !validate_image_content_type(&content_type) {
            return Err(AppError::unprocessable(
                "thumbnail must be jpeg, png, webp, or gif",
            ));
        }
        if data.len() > MAX_THUMBNAIL_BYTES {
            return Err(AppError::unprocessable(
                "thumbnail exceeds 10 MB size limit",
            ));
        }

        let thread = self.find_live_thread(thread_id).await?;
        Self::require_author_or_mod(actor, &thread)?;

        let key = cas_key("thumbnails", &data, &content_type);

        // CAS: upsert or increment ref
        if self.stored_files.exists(&key).await? {
            self.stored_files.increment_ref(&key).await?;
        } else {
            self.stored_files
                .upsert(
                    &key,
                    &content_type,
                    &data,
                    data.len() as i64,
                    Some(actor.id),
                )
                .await?;
        }

        // Release old ref
        let old_key = self.threads.find_thumbnail_key(thread_id).await?;
        self.threads.set_thumbnail(thread_id, key.clone()).await?;

        if let Some(old) = old_key.filter(|k| k != &key) {
            let remaining = self.stored_files.decrement_ref(&old).await?;
            if remaining == 0 {
                self.jobs
                    .enqueue(ForumJob::GcStorageKey { key: old })
                    .await?;
            }
        }

        Ok(format!("/files/{key}"))
    }

    pub async fn remove_thumbnail(
        &self,
        actor: &AuthUser,
        thread_id: Uuid,
    ) -> Result<(), AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let thread = self.find_live_thread(thread_id).await?;
        Self::require_author_or_mod(actor, &thread)?;

        if let Some(key) = self.threads.find_thumbnail_key(thread_id).await? {
            let remaining = self.stored_files.decrement_ref(&key).await?;
            if remaining == 0 {
                self.jobs.enqueue(ForumJob::GcStorageKey { key }).await?;
            }
        }

        self.threads.remove_thumbnail(thread_id).await
    }
}

#[derive(Debug)]
pub struct CreateThreadCmd {
    pub category_id: Uuid,
    pub title: String,
}
