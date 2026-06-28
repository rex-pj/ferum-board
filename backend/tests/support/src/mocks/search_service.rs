use async_trait::async_trait;
use mockall::mock;

use ferum_application::ports::{SearchQuery, SearchResults, SearchService};
use ferum_domain::AppError;

mock! {
    pub SearchService {}
    #[async_trait]
    impl SearchService for SearchService {
        async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError>;
    }
}

/// Always returns empty results — useful when the test doesn't care about search output.
pub struct NoopSearchService;

#[async_trait]
impl SearchService for NoopSearchService {
    async fn search(&self, _query: SearchQuery) -> Result<SearchResults, AppError> {
        Ok(SearchResults { hits: vec![], total: 0 })
    }
}
