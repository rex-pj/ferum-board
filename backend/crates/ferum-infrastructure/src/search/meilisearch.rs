use async_trait::async_trait;
use meilisearch_sdk::client::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_application::ports::{SearchHit, SearchQuery, SearchResults, SearchService};
use ferum_application::shared::AppError;

pub struct MeilisearchService {
    client: Client,
    index: String,
}

impl MeilisearchService {
    pub fn new(url: &str, api_key: Option<&str>, index: &str) -> Self {
        let client = Client::new(url, api_key).expect("valid Meilisearch URL");
        Self {
            client,
            index: index.to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ThreadDoc {
    id: String,
    slug: String,
    title: String,
    content: Option<String>,
}

#[async_trait]
impl SearchService for MeilisearchService {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError> {
        if query.q.trim().is_empty() {
            return Ok(SearchResults {
                hits: vec![],
                total: 0,
            });
        }

        let index = self.client.index(&self.index);
        let offset = (query.page.saturating_sub(1)) * query.per_page;

        let mut search = index.search();
        search
            .with_query(&query.q)
            .with_limit(query.per_page as usize)
            .with_offset(offset as usize);

        let filter;
        if let Some(cat_id) = query.category_id {
            filter = format!("category_id = \"{}\"", cat_id);
            search.with_filter(&filter);
        }

        let results = search
            .execute::<ThreadDoc>()
            .await
            .map_err(|e| AppError::internal(format!("Meilisearch error: {}", e)))?;

        let total = results.estimated_total_hits.unwrap_or(0) as u64;
        let hits = results
            .hits
            .into_iter()
            .filter_map(|h| {
                let doc = h.result;
                let thread_id = Uuid::parse_str(&doc.id).ok()?;
                Some(SearchHit {
                    thread_id,
                    thread_slug: doc.slug,
                    title: doc.title,
                    excerpt: doc.content.map(|c| c.chars().take(200).collect()),
                })
            })
            .collect();

        Ok(SearchResults { hits, total })
    }
}
