use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::constants::{DEFAULT_MAX_THREADS_PER_PAGE, DEFAULT_POST_EDIT_WINDOW_HOURS, MAX_TAGS_PER_THREAD, MAX_THUMBNAIL_BYTES};
use crate::event_bus::EventPublisher;
use crate::permission::PermissionChecker;
use crate::ports::{CacheService, ForumJob, HookContext, HookDecision, JobQueue, NullPluginRuntime, PluginHookRuntime};
use crate::shared::{AppError, OptionExt};
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::validate_image_magic;
use crate::validators::{generate_thread_slug, validate_thread_title};
use ferum_domain::events::ForumEvent;
use ferum_domain::models::role::perm;
use ferum_domain::models::thread::{Thread, ThreadStatus};
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::site_config_repository::{
    get_config_i64, get_config_u64, SiteConfigRepository,
};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::tag_repository::TagRepository;
use ferum_domain::repositories::thread_repository::{
    AdminThreadFilter, NewThread, ThreadFilter, ThreadRepository, ThreadSort, UpdateThread,
};
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;

pub struct ThreadUseCase {
    pub threads: Arc<dyn ThreadRepository>,
    pub categories: Arc<dyn CategoryRepository>,
    pub posts: Arc<dyn PostRepository>,
    pub jobs: Arc<dyn JobQueue>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub event_bus: Arc<dyn EventPublisher>,
    pub cache: Arc<dyn CacheService>,
    pub tags: Arc<dyn TagRepository>,
    pub users: Arc<dyn UserRepository>,
    pub plugin_runtime: Arc<dyn PluginHookRuntime>,
    pub site_config: Option<Arc<dyn SiteConfigRepository>>,
    /// When true: dedup views via the thread_view_dedup table (each viewer counts once per thread per day).
    /// When false: every request counts as a view.
    pub dedup_view_counts: bool,
    /// In-memory buffer: batches view count increments and flushes to DB every 60s.
    /// Eliminates per-request hot-row UPDATE on threads.view_count under concurrent load.
    view_count_buffer: Arc<DashMap<Uuid, u32>>,
}

impl ThreadUseCase {
    pub fn new(
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        posts: Arc<dyn PostRepository>,
        jobs: Arc<dyn JobQueue>,
        stored_files: Arc<dyn StoredFileRepository>,
        event_bus: Arc<dyn EventPublisher>,
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
            site_config: None,
            dedup_view_counts: false,
            view_count_buffer: Arc::new(DashMap::new()),
        }
    }

    pub fn with_dedup_view_counts(mut self, enabled: bool) -> Self {
        self.dedup_view_counts = enabled;
        self
    }

    /// Drains the in-memory view count buffer and flushes accumulated counts to the DB.
    /// Called by a background task in startup.rs every 60 seconds.
    /// On server restart, buffered counts not yet flushed are lost — acceptable for view counts.
    pub async fn flush_view_counts(&self) -> Result<(), AppError> {
        // Atomically drain each key. Entries added to already-visited keys during
        // DB writes are captured by remove(); entries on brand-new keys stay in
        // the buffer for the next flush cycle.
        let snapshot: Vec<(Uuid, u32)> = self
            .view_count_buffer
            .iter()
            .map(|e| *e.key())
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|k| self.view_count_buffer.remove(&k).map(|(k, v)| (k, v)))
            .collect();

        for (thread_id, count) in snapshot {
            if count == 0 {
                continue;
            }
            if let Err(e) = self.threads.add_view_count(thread_id, count as i32).await {
                tracing::warn!("view count flush failed for {}: {:?}", thread_id, e);
            }
        }
        Ok(())
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginHookRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    pub fn with_site_config(mut self, site_config: Arc<dyn SiteConfigRepository>) -> Self {
        self.site_config = Some(site_config);
        self
    }

    async fn max_threads_per_page(&self) -> u64 {
        match &self.site_config {
            Some(sc) => get_config_u64(sc.as_ref(), "max_threads_per_page", DEFAULT_MAX_THREADS_PER_PAGE).await,
            None => DEFAULT_MAX_THREADS_PER_PAGE,
        }
    }

    async fn post_edit_window_hours(&self) -> i64 {
        match &self.site_config {
            Some(sc) => get_config_i64(sc.as_ref(), "post_edit_window_hours", DEFAULT_POST_EDIT_WINDOW_HOURS).await,
            None => DEFAULT_POST_EDIT_WINDOW_HOURS,
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    async fn find_live_thread(&self, id: Uuid) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .or_not_found()?;
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

    /// Before-hook: allow plugins to inspect or block thread deletion.
    /// Shared by both delete entry points (`soft_delete` and `delete_by_slug`).
    async fn dispatch_before_thread_delete(
        &self,
        actor: &AuthUser,
        thread: &Thread,
    ) -> Result<(), AppError> {
        let hook_ctx = HookContext {
            hook_name: "before_thread_delete".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "thread_id": thread.id,
                "category_id": thread.category_id,
                "author_id": thread.author_id,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_thread_delete", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                Err(AppError::PluginBlocked { reason, error_code })
            }
            HookDecision::Allow => Ok(()),
        }
    }

    #[tracing::instrument(skip_all, fields(cache_hit = tracing::field::Empty))]
    async fn visible_category_map(
        &self,
        actor: Option<&AuthUser>,
    ) -> Result<HashMap<Uuid, (String, String)>, AppError> {
        const CACHE_KEY: &str = "forum:categories:all";

        let all_categories = if let Some(cached) = self.cache.get(CACHE_KEY).await {
            match serde_json::from_str(&cached) {
                Ok(cats) => {
                    tracing::Span::current().record("cache_hit", true);
                    cats
                }
                Err(_) => {
                    // Corrupt cache entry — fetch from DB and overwrite.
                    tracing::Span::current().record("cache_hit", false);
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
            tracing::Span::current().record("cache_hit", false);
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

    /// TTL for cached thread-list totals. Short enough that a newly created thread is
    /// reflected in the total within seconds, long enough to absorb listing traffic.
    const COUNT_CACHE_TTL: Duration = Duration::from_secs(30);

    async fn read_cached_count(&self, key: &str) -> Option<u64> {
        self.cache.get(key).await.and_then(|s| s.parse::<u64>().ok())
    }

    async fn write_cached_count(&self, key: &str, total: u64) {
        self.cache
            .set(key, &total.to_string(), Self::COUNT_CACHE_TTL)
            .await
            .ok();
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

    #[tracing::instrument(skip_all, fields(category_slug = %category_slug, page = page))]
    pub async fn list_by_category(
        &self,
        actor: Option<&AuthUser>,
        category_slug: &str,
        sort: ThreadSort,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category = self
            .categories
            .find_by_slug(category_slug)
            .await?
            .or_not_found()?;

        PermissionChecker::can_view_category(actor, &category)?;

        let per_page = per_page.min(self.max_threads_per_page().await);
        let filter = ThreadFilter { sort };
        // The category thread count is identical for every viewer (no per-user filter),
        // so cache it briefly and skip the COUNT(*) on the hot path. Sort matters because
        // Unanswered/Solved add WHERE predicates that change the total.
        let count_key = format!("threads:count:cat:{}:{}", category.id, filter.sort.as_str());
        let cached_total = self.read_cached_count(&count_key).await;
        let (mut threads, total) = self
            .threads
            .list_by_category(category.id, &filter, page, per_page, cached_total)
            .await?;
        if cached_total.is_none() {
            self.write_cached_count(&count_key, total).await;
        }
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.category_slug = category.slug.clone();
            t.category_name = Some(category.name.clone());
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    #[tracing::instrument(skip_all, fields(page = page))]
    pub async fn list_feed(
        &self,
        actor: Option<&AuthUser>,
        sort: ThreadSort,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category_map = self.visible_category_map(actor).await?;
        let visible_ids: Vec<Uuid> = category_map.keys().cloned().collect();
        let per_page = per_page.min(self.max_threads_per_page().await);

        // Resolved from the map already in hand — no extra query. `None` means the
        // category does not exist yet (no review has ever been written), in which
        // case there is nothing to exclude.
        let reviews_id: Option<Uuid> = category_map
            .iter()
            .find(|(_, (slug, _))| slug == crate::usecases::category_usecase::REVIEWS_CATEGORY_SLUG)
            .map(|(id, _)| *id);

        // Personalized feed: filter by watched/muted when the user is logged in.
        let (feed_ids, watches_reviews) = if let Some(actor) = actor {
            let (watched, muted) = tokio::try_join!(
                self.users.get_watched_categories(actor.id),
                self.users.get_muted_categories(actor.id),
            )?;

            let had_watched = !watched.is_empty();
            // Watching the reviews category is a deliberate opt-in, so it overrides
            // the blanket exclusion applied further down. Without this, a user who
            // went looking for that toggle and switched it on would see no effect —
            // the worst kind of setting.
            let watches_reviews = reviews_id.is_some_and(|id| watched.contains(&id));

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
            let ids = if filtered.is_empty() && !has_visible_watched {
                category_map.keys()
                    .cloned()
                    .filter(|id| !muted.contains(id))
                    .collect()
            } else {
                filtered
            };
            (ids, watches_reviews)
        } else {
            (visible_ids, false)
        };

        // Final safety fallback: if every visible category is muted by a guest-path
        // (shouldn't happen — guests have no mute list), serve all visible.
        let feed_ids = if feed_ids.is_empty() && actor.is_none() {
            category_map.keys().cloned().collect()
        } else {
            feed_ids
        };

        // Product reviews are kept out of the discussion feed. They are threads and
        // keep every thread behaviour (replies, reactions, reports, search), but they
        // are *catalogue* content: they arrive at a rate driven by products × buyers,
        // so left in the feed they crowd out discussion exactly as the catalogue
        // succeeds. They stay reachable via the product page, /catalog, the reviews
        // category itself, search, and the homepage's "latest reviews" panel.
        //
        // Applied after the fallbacks above so neither can reintroduce the category.
        // Deliberately no "don't empty the feed" guard: if reviews are the only
        // visible category then there genuinely are no discussions, and the empty
        // state says so honestly. `list_feed` treats an empty id list as an empty
        // result, not as "unfiltered", so this cannot widen the query.
        let feed_ids: Vec<Uuid> = match reviews_id {
            Some(id) if !watches_reviews => {
                feed_ids.into_iter().filter(|c| *c != id).collect()
            }
            _ => feed_ids,
        };

        let filter = ThreadFilter { sort };
        // Only cache the count for the guest feed, whose category set is stable. A
        // logged-in user's feed is personalized (watched/muted), so its count is not
        // shared and not worth caching — pass None to compute it normally.
        //
        // The key carries `nr` ("no reviews") because the guest feed's category set
        // changed when reviews were excluded. Reusing the old key would serve a total
        // that still counted review threads against a list that no longer contains
        // them — a paginator promising pages that render empty, and it would fail
        // silently until the 30s TTL expired on every deployed instance.
        let count_key = (actor.is_none())
            .then(|| format!("threads:count:feed:guest:nr:{}", filter.sort.as_str()));
        let cached_total = match &count_key {
            Some(k) => self.read_cached_count(k).await,
            None => None,
        };
        let (mut threads, total) = self
            .threads
            .list_feed(&feed_ids, &filter, page, per_page, cached_total)
            .await?;
        if let Some(k) = &count_key {
            if cached_total.is_none() {
                self.write_cached_count(k, total).await;
            }
        }
        Self::enrich_threads(&mut threads, &category_map);
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_threads_for_mod(
        &self,
        actor: &AuthUser,
        filter: AdminThreadFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        if !actor.has_perm_any_category(ferum_domain::models::role::perm::MOD_VIEW_REPORTS)
            && !actor.has_perm(ferum_domain::models::role::perm::ADMIN_USERS)
        {
            return Err(AppError::forbidden("permission_denied"));
        }
        let per_page = per_page.min(self.max_threads_per_page().await);
        let category_map = self.visible_category_map(Some(actor)).await?;

        let (mut threads, total) = self
            .threads
            .list_admin_threads(&filter, page, per_page)
            .await?;

        Self::enrich_threads(&mut threads, &category_map);
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_threads_for_admin(
        &self,
        actor: &AuthUser,
        filter: AdminThreadFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        PermissionChecker::can_manage_users(actor)?;
        let per_page = per_page.min(self.max_threads_per_page().await);
        let category_map = self.visible_category_map(Some(actor)).await?;

        let (mut threads, total) = self
            .threads
            .list_admin_threads(&filter, page, per_page)
            .await?;

        Self::enrich_threads(&mut threads, &category_map);
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    #[tracing::instrument(skip_all, fields(tag_slug = %tag_slug, page = page))]
    pub async fn list_by_tag(
        &self,
        actor: Option<&AuthUser>,
        tag_slug: &str,
        sort: ThreadSort,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category_map = self.visible_category_map(actor).await?;
        let visible_ids: Vec<Uuid> = category_map.keys().cloned().collect();
        let per_page = per_page.min(self.max_threads_per_page().await);
        let filter = ThreadFilter { sort };
        let (mut threads, total) = self
            .threads
            .list_by_tag(tag_slug, &visible_ids, &filter, page, per_page)
            .await?;
        Self::enrich_threads(&mut threads, &category_map);
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    #[tracing::instrument(skip_all, fields(author_id = %author_id, page = page))]
    pub async fn list_by_author(
        &self,
        actor: Option<&AuthUser>,
        author_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let category_map = self.visible_category_map(actor).await?;
        let visible_ids: Vec<Uuid> = category_map.keys().cloned().collect();
        let per_page = per_page.min(self.max_threads_per_page().await);
        let (mut threads, total) = self
            .threads
            .list_by_author(author_id, &visible_ids, page, per_page)
            .await?;
        Self::enrich_threads(&mut threads, &category_map);
        let thread_ids: Vec<Uuid> = threads.iter().map(|t| t.id).collect();
        let tag_map = self.tags.find_by_threads(&thread_ids).await.unwrap_or_default();
        for t in threads.iter_mut() {
            t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
        }
        Ok((threads, total))
    }

    #[tracing::instrument(skip_all, fields(slug = %slug))]
    pub async fn get_by_slug(
        &self,
        actor: Option<&AuthUser>,
        slug: &str,
        guest_key: Option<&str>,
    ) -> Result<Thread, AppError> {
        let mut thread = self
            .threads
            .find_by_slug(slug)
            .await?
            .or_not_found()?;

        if thread.status == ThreadStatus::Deleted {
            return Err(AppError::NotFound);
        }

        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .or_not_found()?;

        PermissionChecker::can_view_category(actor, &category)?;

        // Fire-and-forget: view count does not block the response.
        // Buffer accumulates increments; background task (startup.rs, 60s) flushes to DB.
        let threads_clone = self.threads.clone();
        let buf = self.view_count_buffer.clone();
        let thread_id = thread.id;

        if self.dedup_view_counts {
            // Derive viewer key and type from authenticated user or guest fingerprint.
            let (viewer_key, viewer_type): (Option<String>, &'static str) = match actor {
                Some(user) => (Some(user.id.to_string()), "user"),
                None => (guest_key.map(str::to_owned), "guest"),
            };

            if let Some(key) = viewer_key {
                tokio::spawn(async move {
                    match threads_clone.try_record_view(thread_id, &key, viewer_type).await {
                        Ok(true) => {
                            *buf.entry(thread_id).or_insert(0) += 1;
                        }
                        Ok(false) => {}
                        Err(e) => tracing::warn!("view dedup record failed for {}: {:?}", thread_id, e),
                    }
                });
            }
            // Guest with no fingerprint (missing header) — skip, do not count.
        } else {
            tokio::spawn(async move {
                *buf.entry(thread_id).or_insert(0) += 1;
            });
        }

        thread.category_slug = category.slug;
        thread.tags = self.tags.find_by_thread(thread.id).await.unwrap_or_default();
        Ok(thread)
    }

    #[tracing::instrument(skip(self), fields(thread_id = %id))]
    pub async fn get_by_id(&self, id: Uuid) -> Result<Thread, AppError> {
        let thread = self
            .threads
            .find_by_id(id)
            .await?
            .or_not_found()?;
        if thread.status == ThreadStatus::Deleted {
            return Err(AppError::NotFound);
        }
        Ok(thread)
    }

    /// Latest reviews across the whole catalogue, at most one per product.
    ///
    /// Feeds the homepage panel that replaces reviews in the discussion feed: the
    /// feed no longer carries them, so this is how a new review still reaches the
    /// front page. See [`ThreadRepository::list_latest_reviews`] for why the
    /// per-product collapse happens in SQL.
    pub async fn latest_reviews(&self, limit: u64) -> Result<Vec<Thread>, AppError> {
        self.threads.list_latest_reviews(limit).await
    }

    /// Author-enriched review threads for one product (public — newest first).
    pub async fn list_reviews_for_product(
        &self,
        product_id: Uuid,
        limit: u64,
    ) -> Result<Vec<Thread>, AppError> {
        self.threads.list_by_product(product_id, limit).await
    }

    /// This author's existing review of the product, if they have written one.
    ///
    /// Lets the product page offer "edit your review" instead of a Write button
    /// that would 409 — the one-review-per-author rule made visible before the
    /// user commits to filling in a form.
    pub async fn find_review_by_author(
        &self,
        product_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<Thread>, AppError> {
        self.threads.find_review_by_author(product_id, author_id).await
    }

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id, category_id = %cmd.category_id))]
    pub async fn create(&self, actor: &AuthUser, cmd: CreateThreadCmd) -> Result<Thread, AppError> {
        let category = self
            .categories
            .find_by_id(cmd.category_id)
            .await?
            .or_not_found()?;

        PermissionChecker::can_create_post(actor, &category)?;

        if !validate_thread_title(&cmd.title) {
            return Err(AppError::invalid("thread_title_length"));
        }

        // One review per author per product. `uq_threads_product_author` is the
        // real guarantee — this check exists so the caller gets a 409 it can act
        // on rather than an opaque unique-violation surfacing as a 500. The race
        // between the two is fine: the index still wins, and losing it costs a
        // failed insert, not a duplicate rating.
        if let Some(product_id) = cmd.product_id {
            if self
                .threads
                .find_review_by_author(product_id, actor.id)
                .await?
                .is_some()
            {
                return Err(AppError::Conflict("product_already_reviewed".to_string()));
            }
        }

        // Pre-validate tag permissions before creating the thread to avoid partial creation:
        // if a requested tag doesn't exist yet and actor lacks tag.create, fail early
        // rather than leaving an orphaned thread row with no posts.
        if !cmd.tag_names.is_empty() && !actor.has_perm(perm::TAG_CREATE) {
            for raw in cmd.tag_names.iter().take(MAX_TAGS_PER_THREAD) {
                let name = raw.trim().to_string();
                if name.is_empty() {
                    continue;
                }
                let tag_slug = slug::slugify(&name);
                if self.tags.find_by_slug(&tag_slug).await?.is_none() {
                    return Err(AppError::forbidden("tag_create_permission_required"));
                }
            }
        }

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
                product_id: cmd.product_id,
            })
            .await?;

        if !cmd.tag_names.is_empty() {
            let mut tag_ids = Vec::new();
            for raw in cmd.tag_names.iter().take(MAX_TAGS_PER_THREAD) {
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

    #[tracing::instrument(skip(self, actor, title), fields(user_id = %actor.id, thread_id = %id))]
    pub async fn update_title(
        &self,
        actor: &AuthUser,
        id: Uuid,
        title: String,
    ) -> Result<Thread, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let thread = self.find_live_thread(id).await?;

        if thread.status == ThreadStatus::Locked
            && !actor.has_perm_in(perm::THREAD_EDIT_ANY, thread.category_id)
        {
            return Err(AppError::forbidden("thread_locked"));
        }

        let is_author = thread.author_id == actor.id;
        let is_author_within_window = is_author
            && Utc::now() - thread.created_at <= chrono::Duration::hours(self.post_edit_window_hours().await);
        let has_mod_perm = actor.has_perm_in(perm::THREAD_EDIT_ANY, thread.category_id);

        if !is_author_within_window && !has_mod_perm {
            return Err(if is_author {
                AppError::forbidden("edit_window_expired")
            } else {
                AppError::forbidden("not_author")
            });
        }

        if !validate_thread_title(&title) {
            return Err(AppError::invalid("thread_title_length"));
        }

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

    /// Replace the tag set of a thread.  Author within edit window or any mod can do this.
    #[tracing::instrument(skip(self, actor, tag_names), fields(user_id = %actor.id, thread_id = %thread_id))]
    pub async fn update_tags(
        &self,
        actor: &AuthUser,
        thread_id: Uuid,
        tag_names: Vec<String>,
    ) -> Result<(), AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let thread = self.find_live_thread(thread_id).await?;

        if thread.status == ThreadStatus::Locked
            && !actor.has_perm_in(perm::THREAD_EDIT_ANY, thread.category_id)
        {
            return Err(AppError::forbidden("thread_locked"));
        }

        let is_author_within_window = thread.author_id == actor.id
            && Utc::now() - thread.created_at <= chrono::Duration::hours(self.post_edit_window_hours().await);

        if !is_author_within_window
            && !actor.has_perm_in(perm::THREAD_EDIT_ANY, thread.category_id)
        {
            return Err(AppError::forbidden("edit_window_expired"));
        }

        let mut tag_ids: Vec<uuid::Uuid> = Vec::new();
        for raw in tag_names.iter().take(MAX_TAGS_PER_THREAD) {
            let name = raw.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let tag_slug = slug::slugify(&name);
            let tag = match self.tags.find_by_slug(&tag_slug).await? {
                Some(t) => t,
                None => {
                    if !actor.has_perm(perm::TAG_CREATE) {
                        continue;
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

        self.tags.replace_thread_tags(thread_id, &tag_ids).await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, thread_id = %id))]
    pub async fn soft_delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let thread = self.find_live_thread(id).await?;
        Self::require_author_or_mod(actor, &thread)?;
        PermissionChecker::require_not_banned(actor)?;

        self.dispatch_before_thread_delete(actor, &thread).await?;

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

    /// Slug-based delete for the HTTP DELETE handler.
    /// Skips category visibility check — thread ownership/delete permission is sufficient.
    /// No view count side effects unlike get_by_slug.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, slug = %slug))]
    pub async fn delete_by_slug(&self, actor: &AuthUser, slug: &str) -> Result<(), AppError> {
        let thread = self.threads.find_by_slug(slug).await?.or_not_found()?;
        if thread.status == ThreadStatus::Deleted {
            return Err(AppError::NotFound);
        }
        Self::require_author_or_mod(actor, &thread)?;
        PermissionChecker::require_not_banned(actor)?;

        self.dispatch_before_thread_delete(actor, &thread).await?;

        self.threads
            .update(
                thread.id,
                UpdateThread {
                    status: Some(ThreadStatus::Deleted),
                    deleted_by_id: Some(actor.id),
                    deleted_at: Some(Utc::now()),
                    ..Default::default()
                },
            )
            .await?;

        self.release_attachment_refs(thread.id).await;

        self.event_bus
            .publish(ForumEvent::ThreadDeleted {
                thread_id: thread.id,
                deleted_by_id: actor.id,
            })
            .await;

        Ok(())
    }

    /// Un-publishes every post attachment the thread's posts still reference.
    ///
    /// Deleting a thread only flips the thread's own status; its post rows are
    /// left intact, so without this their images would stay publicly servable
    /// after the thread containing them is gone.
    ///
    /// Reachable only once per thread — `delete_by_slug` rejects an already
    /// deleted thread — so these decrements cannot be applied twice. As in
    /// `PostUseCase`, a count reaching zero un-publishes the blob but never
    /// deletes it, and bookkeeping failures are logged rather than failing the
    /// delete the user actually asked for.
    async fn release_attachment_refs(&self, thread_id: Uuid) {
        let contents = match self.posts.content_md_by_thread(thread_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(%thread_id, "could not load posts to release attachments: {e:?}");
                return;
            }
        };

        // Per post, not deduplicated across posts: two posts embedding the same
        // image took two references, so both have to be given back.
        for content in &contents {
            for key in crate::usecases::post_usecase::extract_attachment_keys(content) {
                if let Err(e) = self.stored_files.decrement_ref(&key).await {
                    tracing::warn!(attachment_key = %key, "attachment decrement_ref failed: {e:?}");
                }
            }
        }
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, thread_id = %id, pin = pin))]
    pub async fn pin(
        &self,
        actor: &AuthUser,
        id: Uuid,
        pin: bool,
    ) -> Result<Thread, AppError> {
        let thread = self.find_live_thread(id).await?;
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

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, thread_id = %id, lock = lock))]
    pub async fn lock(
        &self,
        actor: &AuthUser,
        id: Uuid,
        lock: bool,
    ) -> Result<Thread, AppError> {
        let thread = self.find_live_thread(id).await?;
        PermissionChecker::can_lock(actor, thread.category_id)?;
        let status = if lock { ThreadStatus::Locked } else { ThreadStatus::Open };
        let result = self
            .threads
            .update(
                id,
                UpdateThread {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await?;
        if lock {
            self.event_bus
                .publish(ForumEvent::ThreadLocked {
                    thread_id: id,
                    by_user_id: actor.id,
                })
                .await;
        }
        Ok(result)
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, thread_id = %id, target_category_id = %target_category_id))]
    pub async fn move_to(
        &self,
        actor: &AuthUser,
        id: Uuid,
        target_category_id: Uuid,
    ) -> Result<Thread, AppError> {
        let thread = self.find_live_thread(id).await?;
        PermissionChecker::can_move(actor, thread.category_id)?;
        self.categories
            .find_by_id(target_category_id)
            .await?
            .or_not_found()?;
        let from_category = thread.category_id;
        let result = self
            .threads
            .update(
                id,
                UpdateThread {
                    category_id: Some(target_category_id),
                    ..Default::default()
                },
            )
            .await?;
        self.event_bus
            .publish(ForumEvent::ThreadMoved {
                thread_id: id,
                from_category,
                to_category: target_category_id,
                by_user_id: actor.id,
            })
            .await;
        Ok(result)
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, thread_id = %id, best_answer_id = %best_answer_id))]
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
            .or_not_found()?;
        if best_post.thread_id != id {
            return Err(AppError::invalid("best_answer_wrong_thread"));
        }

        // Idempotent no-op if this exact answer is already marked as best — avoids
        // double-firing the event and double-rewarding trust score on a client retry.
        if thread.is_solved && thread.best_answer_id == Some(best_answer_id) {
            return Ok(thread);
        }

        let hook_ctx = HookContext {
            hook_name: "before_best_answer_mark".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "thread_id": id,
                "category_id": thread.category_id,
                "post_id": best_answer_id,
                "post_author_id": best_post.author_id,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_best_answer_mark", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
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
                thread_title: thread.title,
                post_author_id: best_post.author_id,
                by_user_id: actor.id,
                by_username: actor.username.clone(),
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

    #[tracing::instrument(skip(self, actor, data, content_type), fields(user_id = %actor.id, thread_id = %thread_id))]
    pub async fn set_thumbnail(
        &self,
        actor: &AuthUser,
        thread_id: Uuid,
        data: bytes::Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::can_upload(actor)?;

        if !validate_image_content_type(&content_type) || !validate_image_magic(&data) {
            return Err(AppError::invalid("thumbnail_invalid_type"));
        }
        if data.len() > MAX_THUMBNAIL_BYTES {
            return Err(AppError::invalid_with("thumbnail_too_large", [("limit_mb", (MAX_THUMBNAIL_BYTES / (1024 * 1024)).into())]));
        }

        let thread = self.find_live_thread(thread_id).await?;
        Self::require_author_or_mod(actor, &thread)?;

        let key = cas_key("thumbnails", &data, &content_type);

        self.stored_files
            .upsert_and_ref(&key, &content_type, &data, data.len() as i64, Some(actor.id))
            .await?;

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

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, thread_id = %thread_id))]
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
    /// Optional product link — makes the thread a structured product review.
    pub product_id: Option<Uuid>,
}
