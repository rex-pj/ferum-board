use std::collections::HashMap;
use std::sync::Arc;

use crate::constants::FORUM_INDEX_THREADS_PER_CATEGORY;
use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::models::category::Category;
use ferum_domain::models::thread::Thread;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::tag_repository::TagRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::AuthUser;

// ─── Forum index types ────────────────────────────────────────────────────────

pub struct SubcategoryCount {
    pub category: Category,
    pub thread_count: u64,
}

pub struct ForumIndexItem {
    pub category: Category,
    pub thread_count: u64,
    pub subcategories: Vec<SubcategoryCount>,
    pub recent_threads: Vec<Thread>,
}

// ─── Use case ─────────────────────────────────────────────────────────────────

pub struct CategoryUseCase {
    pub categories: Arc<dyn CategoryRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub tags: Arc<dyn TagRepository>,
}

impl CategoryUseCase {
    pub fn new(
        categories: Arc<dyn CategoryRepository>,
        threads: Arc<dyn ThreadRepository>,
        tags: Arc<dyn TagRepository>,
    ) -> Self {
        Self {
            categories,
            threads,
            tags,
        }
    }

    /// Public listing — filters by view_policy relative to the calling user.
    pub async fn list_visible(&self, actor: Option<&AuthUser>) -> Result<Vec<Category>, AppError> {
        let all = self.categories.list_all().await?;
        let visible = all
            .into_iter()
            .filter(|c| PermissionChecker::can_view_category(actor, c).is_ok())
            .collect();
        Ok(visible)
    }

    pub async fn get_by_slug(
        &self,
        actor: Option<&AuthUser>,
        slug: &str,
    ) -> Result<Category, AppError> {
        let category = self
            .categories
            .find_by_slug(slug)
            .await?
            .ok_or(AppError::NotFound)?;

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
        let (counts_vec, recent_threads) = tokio::try_join!(
            self.categories.count_threads_by_categories(&all_ids),
            self.threads
                .list_recent_by_categories(&parent_ids, FORUM_INDEX_THREADS_PER_CATEGORY),
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
}
