use std::sync::Arc;

use uuid::Uuid;

use crate::ports::{SearchQuery, SearchResults, SearchService};
use crate::shared::AppError;

pub struct SearchUseCase {
    pub search: Arc<dyn SearchService>,
}

impl SearchUseCase {
    pub fn new(search: Arc<dyn SearchService>) -> Self {
        Self { search }
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
}
