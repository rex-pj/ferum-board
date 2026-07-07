use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ports::{SearchQuery, SearchResults, SearchService};
use crate::shared::AppError;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;

/// A search hit joined with live thread + category data, for SSR result pages.
/// Meta fields are `None` when the indexed thread no longer resolves (e.g. it
/// was deleted after indexing) — the hit itself is still shown.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HydratedSearchHit {
    pub thread_id: Uuid,
    pub thread_slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub reply_count: i32,
}

pub struct SearchUseCase {
    pub search: Arc<dyn SearchService>,
    pub threads: Arc<dyn ThreadRepository>,
    pub categories: Arc<dyn CategoryRepository>,
}

impl SearchUseCase {
    pub fn new(
        search: Arc<dyn SearchService>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
    ) -> Self {
        Self {
            search,
            threads,
            categories,
        }
    }

    pub async fn search(
        &self,
        q: String,
        category_ids: Vec<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<SearchResults, AppError> {
        if q.trim().is_empty() {
            return Ok(SearchResults {
                hits: vec![],
                total: 0,
            });
        }

        self.search
            .search(SearchQuery {
                q,
                category_ids,
                page,
                per_page: per_page.min(30),
            })
            .await
    }

    /// Same as [`Self::search`] but joins each hit with its thread's author,
    /// category and stats so result lists can show full context. Hydration is
    /// best-effort: a lookup failure degrades to bare hits, never an error.
    pub async fn search_hydrated(
        &self,
        q: String,
        category_ids: Vec<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<HydratedSearchHit>, u64), AppError> {
        let results = self.search(q, category_ids, page, per_page).await?;
        if results.hits.is_empty() {
            return Ok((vec![], results.total));
        }

        let ids: Vec<Uuid> = results.hits.iter().map(|h| h.thread_id).collect();
        let threads = self.threads.find_many_by_ids(&ids).await.unwrap_or_default();
        let categories = self.categories.list_all().await.unwrap_or_default();

        let thread_map: HashMap<Uuid, _> = threads.iter().map(|t| (t.id, t)).collect();
        let cat_map: HashMap<Uuid, _> = categories.iter().map(|c| (c.id, c)).collect();

        // Preserve the relevance order of the raw hits.
        let hits = results
            .hits
            .iter()
            .map(|h| {
                let thread = thread_map.get(&h.thread_id);
                let category = thread.and_then(|t| cat_map.get(&t.category_id));
                HydratedSearchHit {
                    thread_id: h.thread_id,
                    thread_slug: h.thread_slug.clone(),
                    title: h.title.clone(),
                    excerpt: h.excerpt.clone(),
                    category_slug: category.map(|c| c.slug.clone()),
                    category_name: category.map(|c| c.name.clone()),
                    author_username: thread.and_then(|t| t.author_username.clone()),
                    author_display_name: thread.and_then(|t| {
                        t.author_display_name
                            .clone()
                            .or_else(|| t.author_username.clone())
                    }),
                    created_at: thread.map(|t| t.created_at),
                    reply_count: thread.map(|t| t.reply_count).unwrap_or(0),
                }
            })
            .collect();

        Ok((hits, results.total))
    }
}
