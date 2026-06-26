use std::collections::HashMap;
use std::sync::Arc;

use crate::constants::DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY;
use crate::dto::{ForumIndexItem, SubcategoryCount};
use crate::permission::PermissionChecker;
use crate::shared::{AppError, OptionExt};
use ferum_domain::models::category::Category;
use ferum_domain::models::thread::Thread;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::site_config_repository::{get_config_u64, SiteConfigRepository};
use ferum_domain::repositories::tag_repository::TagRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;
use uuid::Uuid;

// ─── Use case ─────────────────────────────────────────────────────────────────

pub struct CategoryUseCase {
    pub categories: Arc<dyn CategoryRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub tags: Arc<dyn TagRepository>,
    pub site_config: Option<Arc<dyn SiteConfigRepository>>,
    users: Arc<dyn UserRepository>,
}

impl CategoryUseCase {
    pub fn new(
        categories: Arc<dyn CategoryRepository>,
        threads: Arc<dyn ThreadRepository>,
        tags: Arc<dyn TagRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            categories,
            threads,
            tags,
            site_config: None,
            users,
        }
    }

    pub fn with_site_config(mut self, site_config: Arc<dyn SiteConfigRepository>) -> Self {
        self.site_config = Some(site_config);
        self
    }

    /// Public listing — filters by view_policy relative to the calling user.
    #[tracing::instrument(skip_all)]
    pub async fn list_visible(&self, actor: Option<&AuthUser>) -> Result<Vec<Category>, AppError> {
        let all = self.categories.list_all().await?;
        let visible = all
            .into_iter()
            .filter(|c| PermissionChecker::can_view_category(actor, c).is_ok())
            .collect();
        Ok(visible)
    }

    #[tracing::instrument(skip(self, actor))]
    pub async fn get_by_slug(
        &self,
        actor: Option<&AuthUser>,
        slug: &str,
    ) -> Result<Category, AppError> {
        let category = self
            .categories
            .find_by_slug(slug)
            .await?
            .or_not_found()?;

        PermissionChecker::can_view_category(actor, &category)?;
        Ok(category)
    }

    /// Builds the forum index: all visible top-level categories, each with their
    /// visible subcategories, per-category thread counts, and up to
    /// `FORUM_INDEX_THREADS_PER_CATEGORY` recent threads.
    ///
    /// Uses 3 DB round-trips:
    ///   1. list_all (categories)
    ///   2. count_threads_by_categories (one aggregation query)
    ///   3. list_recent_by_categories (one window-function query for threads)
    #[tracing::instrument(skip_all)]
    pub async fn get_forum_index(
        &self,
        actor: Option<&AuthUser>,
    ) -> Result<Vec<ForumIndexItem>, AppError> {
        let all = self.categories.list_all().await?;
        let visible: Vec<Category> = all
            .into_iter()
            .filter(|c| PermissionChecker::can_view_category(actor, c).is_ok())
            .collect();

        if visible.is_empty() {
            return Ok(vec![]);
        }

        let all_ids: Vec<uuid::Uuid> = visible.iter().map(|c| c.id).collect();
        let parent_ids: Vec<uuid::Uuid> = visible
            .iter()
            .filter(|c| c.parent_id.is_none())
            .map(|c| c.id)
            .collect();

        // Fetch thread counts and recent threads concurrently.
        let index_threads_count = match &self.site_config {
            Some(sc) => get_config_u64(sc.as_ref(), "forum_index_threads_per_category", DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY).await,
            None => DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY,
        };
        let (counts_vec, recent_threads) = tokio::try_join!(
            self.categories.count_threads_by_categories(&all_ids),
            self.threads
                .list_recent_by_categories(&parent_ids, index_threads_count),
        )?;

        let counts: HashMap<uuid::Uuid, u64> = counts_vec.into_iter().collect();

        // Group threads by category_id.
        let mut threads_by_cat: HashMap<uuid::Uuid, Vec<Thread>> = HashMap::new();
        for thread in recent_threads {
            threads_by_cat
                .entry(thread.category_id)
                .or_default()
                .push(thread);
        }

        // Group subcategories by parent_id.
        let (parents, subs): (Vec<Category>, Vec<Category>) =
            visible.into_iter().partition(|c| c.parent_id.is_none());

        let mut subs_by_parent: HashMap<uuid::Uuid, Vec<Category>> = HashMap::new();
        for sub in subs {
            if let Some(pid) = sub.parent_id {
                subs_by_parent.entry(pid).or_default().push(sub);
            }
        }

        let items = parents
            .into_iter()
            .map(|cat| {
                let cat_id = cat.id;
                let cat_slug = cat.slug.clone();
                let cat_name = cat.name.clone();

                let subcategories = subs_by_parent
                    .remove(&cat_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| SubcategoryCount {
                        thread_count: counts.get(&s.id).copied().unwrap_or(0),
                        category: s,
                    })
                    .collect();

                // Enrich threads with category slug/name now that we have the context.
                let recent_threads = threads_by_cat
                    .remove(&cat_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut t| {
                        t.category_slug = cat_slug.clone();
                        t.category_name = Some(cat_name.clone());
                        t
                    })
                    .collect();

                ForumIndexItem {
                    thread_count: counts.get(&cat_id).copied().unwrap_or(0),
                    subcategories,
                    recent_threads,
                    category: cat,
                }
            })
            .collect::<Vec<_>>();

        // Enrich all recent threads with tags in one batch query.
        let all_thread_ids: Vec<uuid::Uuid> = items
            .iter()
            .flat_map(|g| g.recent_threads.iter().map(|t| t.id))
            .collect();

        if all_thread_ids.is_empty() {
            return Ok(items);
        }

        let tag_map = self
            .tags
            .find_by_threads(&all_thread_ids)
            .await
            .unwrap_or_default();

        let items = items
            .into_iter()
            .map(|mut group| {
                for t in group.recent_threads.iter_mut() {
                    t.tags = tag_map.get(&t.id).cloned().unwrap_or_default();
                }
                group
            })
            .collect();

        Ok(items)
    }

    // ─── Watch / Mute ─────────────────────────────────────────────────────────

    async fn require_visible(&self, actor: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        let category = self
            .categories
            .find_by_id(category_id)
            .await?
            .or_not_found()?;
        PermissionChecker::can_view_category(Some(actor), &category)
    }

    pub async fn watch_category(&self, actor: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        self.require_visible(actor, category_id).await?;
        self.users.watch_category(actor.id, category_id).await
    }

    pub async fn unwatch_category(&self, actor: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        self.require_visible(actor, category_id).await?;
        self.users.unwatch_category(actor.id, category_id).await
    }

    pub async fn mute_category(&self, actor: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        self.require_visible(actor, category_id).await?;
        self.users.mute_category(actor.id, category_id).await
    }

    pub async fn unmute_category(&self, actor: &AuthUser, category_id: Uuid) -> Result<(), AppError> {
        self.require_visible(actor, category_id).await?;
        self.users.unmute_category(actor.id, category_id).await
    }

    pub async fn get_watch_status(
        &self,
        actor: &AuthUser,
        category_id: Uuid,
    ) -> Result<(bool, bool), AppError> {
        self.require_visible(actor, category_id).await?;
        let (watched, muted) = tokio::try_join!(
            self.users.get_watched_categories(actor.id),
            self.users.get_muted_categories(actor.id),
        )?;
        Ok((watched.contains(&category_id), muted.contains(&category_id)))
    }
}
