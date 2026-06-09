use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::constants::{MAX_THREADS_PER_PAGE, MAX_THUMBNAIL_BYTES, POST_EDIT_WINDOW_HOURS};
use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::ports::{CacheService, ForumJob, HookContext, HookDecision, JobQueue, NullPluginRuntime, PluginRuntime};
use crate::shared::AppError;
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::generate_thread_slug;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::role::perm;
use ferum_domain::models::thread::{Thread, ThreadStatus};
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::tag_repository::TagRepository;
use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository, UpdateThread};
use ferum_domain::repositories::user_repository::UserRepository;
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
    pub tags: Arc<dyn TagRepository>,
    pub users: Arc<dyn UserRepository>,
    pub plugin_runtime: Arc<dyn PluginRuntime>,
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
        tags: Arc<dyn TagRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            threads,
            categories,
            posts,
            jobs,
            stored_files,
            event_bus,
            cache,
            tags,
            users,
            plugin_runtime: Arc::new(NullPluginRuntime),
        }
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
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
        if thread.author_id != actor.id
            && !actor.has_perm_in(perm::THREAD_DELETE_ANY, thread.category_id)
        {
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
        let (mut threads, total) = self
            .threads
            .list_by_category(category.id, page, per_page)
            .await?;
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.category_slug = category.slug.clone();
            t.category_name = Some(category.name.clone());
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
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

        // Personalized feed: filter by watched/muted when the user is logged in.
        let feed_ids = if let Some(actor) = actor {
            let (watched, muted) = tokio::try_join!(
                self.users.get_watched_categories(actor.id),
                self.users.get_muted_categories(actor.id),
            )?;

            let had_watched = !watched.is_empty();

            // watched takes priority — show only watched (minus muted) if any are set.
            // fall back to all visible categories minus muted when no watched set.
            let base: Vec<Uuid> = if had_watched {
                watched.into_iter().filter(|id| visible_ids.contains(id)).collect()
            } else {
                visible_ids.clone()
            };

            // Track whether the user had watched categories that are still visible.
            // Used to distinguish "muted everything" from "all watched were deleted".
            let has_visible_watched = had_watched && !base.is_empty();

            let filtered: Vec<Uuid> = base.into_iter().filter(|id| !muted.contains(id)).collect();

            // Fallback to all visible only when there was no effective personalization
            // (no watched set, or all watched categories became invisible/deleted).
            // Never fall back when the user deliberately muted all their watched categories.
            if filtered.is_empty() && !has_visible_watched {
                category_map.keys()
                    .cloned()
                    .filter(|id| !muted.contains(id))
                    .collect()
            } else {
                filtered
            }
        } else {
            visible_ids
        };

        // Final safety fallback: if every visible category is muted by a guest-path
        // (shouldn't happen — guests have no mute list), serve all visible.
        let feed_ids = if feed_ids.is_empty() && actor.is_none() {
            category_map.keys().cloned().collect()
        } else {
            feed_ids
        };

        let (mut threads, total) = self.threads.list_feed(&feed_ids, page, per_page).await?;
        Self::enrich_threads(&mut threads, &category_map);
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    pub async fn list_by_tag(
        &self,
        actor: Option<&AuthUser>,
        tag_slug: &str,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category_map = self.visible_category_map(actor).await?;
        let visible_ids: Vec<Uuid> = category_map.keys().cloned().collect();
        let per_page = per_page.min(MAX_THREADS_PER_PAGE);
        let (mut threads, total) = self
            .threads
            .list_by_tag(tag_slug, &visible_ids, page, per_page)
            .await?;
        Self::enrich_threads(&mut threads, &category_map);
        for t in threads.iter_mut() {
            t.tags = self
                .tags
                .find_by_thread(t.id)
                .await
                .unwrap_or_default();
        }
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
        thread.tags = self.tags.find_by_thread(thread.id).await.unwrap_or_default();
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

        // Before-hook: allow plugins to inspect or block thread creation
        let hook_ctx = HookContext {
            hook_name: "before_thread_create".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "title": cmd.title,
                "content_md": cmd.content_md,
                "tag_names": cmd.tag_names,
                "category_id": cmd.category_id,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_thread_create", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(crate::shared::AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
        }

        // Pre-generate UUID so slug can embed it — collision-free, no retry needed
        let id = uuid::Uuid::new_v4();
        let slug = generate_thread_slug(&cmd.title, &id);

        let thread = self
            .threads
            .create(NewThread {
                id,
                category_id: cmd.category_id,
                author_id: actor.id,
                title: cmd.title,
                slug,
            })
            .await?;

        if !cmd.tag_names.is_empty() {
            let mut tag_ids = Vec::new();
            for raw in cmd.tag_names.iter().take(5) {
                let name = raw.trim().to_string();
                if name.is_empty() {
                    continue;
                }
                let tag_slug = slug::slugify(&name);
                let tag = match self.tags.find_by_slug(&tag_slug).await? {
                    Some(t) => t,
                    None => {
                        if !actor.has_perm(perm::TAG_CREATE) {
                            return Err(AppError::forbidden("tag_create_permission_required"));
                        }
                        self.tags
                            .create(ferum_domain::models::tag::NewTag {
                                id: uuid::Uuid::new_v4(),
                                name,
                                slug: tag_slug,
                                color: None,
                                created_by_id: Some(actor.id),
                            })
                            .await?
                    }
                };
                tag_ids.push(tag.id);
            }
            if !tag_ids.is_empty() {
                self.tags.assign_to_thread(thread.id, &tag_ids).await?;
            }
        }

        self.event_bus
            .publish(ForumEvent::ThreadCreated {
                thread_id: thread.id,
                thread_slug: thread.slug.clone(),
                author_id: actor.id,
                category_id: thread.category_id,
            })
            .await;

        Ok(thread)
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

        if !is_author_within_window
            && !actor.has_perm_in(perm::THREAD_EDIT_ANY, thread.category_id)
        {
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

        self.event_bus
            .publish(ForumEvent::ThreadDeleted {
                thread_id: id,
                deleted_by_id: actor.id,
            })
            .await;

        Ok(())
    }

    pub async fn pin(
        &self,
        actor: &AuthUser,
        id: Uuid,
        pin: bool,
    ) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_pin(actor, thread.category_id)?;
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
        lock: bool,
    ) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_lock(actor, thread.category_id)?;
        let status = if lock { ThreadStatus::Locked } else { ThreadStatus::Open };
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
        target_category_id: Uuid,
    ) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_move(actor, thread.category_id)?;
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
    ) -> Result<Thread, AppError> {
        let thread = self.find_live_thread(id).await?;

        if thread.author_id != actor.id {
            PermissionChecker::can_lock(actor, thread.category_id)?;
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

        // Reward the author of the best answer. Fire-and-forget.
        {
            let users = self.users.clone();
            let author_id = best_post.author_id;
            tokio::spawn(async move {
                let _ = users.increment_trust_score(author_id, 5).await;
            });
        }

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

        // If this DB write fails we roll back the ref we just added so the
        // stored_files row is not orphaned with ref_count > 0 and no referencing thread.
        if let Err(e) = self.threads.set_thumbnail(thread_id, key.clone()).await {
            let remaining = self.stored_files.decrement_ref(&key).await.unwrap_or(1);
            if remaining == 0 {
                self.jobs
                    .enqueue(ForumJob::GcStorageKey { key })
                    .await
                    .ok();
            }
            return Err(e);
        }

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
    pub content_md: String,
    pub tag_names: Vec<String>,
}
