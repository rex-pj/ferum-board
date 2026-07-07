use std::sync::Arc;

use ferum_application::ports::{SearchHit, SearchResults};
use ferum_application::usecases::search_usecase::SearchUseCase;
use ferum_test_support::fixtures::ids;
use ferum_test_support::mocks::category_repository::MockCategoryRepository;
use ferum_test_support::mocks::search_service::{MockSearchService, NoopSearchService};
use ferum_test_support::mocks::thread_repository::MockThreadRepository;

fn build_uc(search: impl ferum_application::ports::SearchService + 'static) -> SearchUseCase {
    // Hydration repos are unused by the plain `search()` path under test —
    // mocks with no expectations panic if touched, which is what we want.
    SearchUseCase::new(
        Arc::new(search),
        Arc::new(MockThreadRepository::new()),
        Arc::new(MockCategoryRepository::new()),
    )
}

// ─── empty query guard ────────────────────────────────────────────────────────

#[tokio::test]
async fn empty_query_returns_empty_results_without_calling_service() {
    // MockSearchService with no expectations: any call would panic
    let uc = build_uc(MockSearchService::new());

    let result = uc.search("".to_string(), vec![], 1, 20).await.expect("empty query succeeds");
    assert_eq!(result.total, 0);
    assert!(result.hits.is_empty());
}

#[tokio::test]
async fn whitespace_only_query_returns_empty_results() {
    let uc = build_uc(MockSearchService::new());

    let result = uc.search("   ".to_string(), vec![], 1, 20).await.expect("whitespace query succeeds");
    assert_eq!(result.total, 0);
    assert!(result.hits.is_empty());
}

// ─── per_page cap ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn per_page_is_capped_at_30() {
    let mut svc = MockSearchService::new();
    // Verify that per_page is always ≤ 30 regardless of caller input
    svc.expect_search()
        .withf(|q| q.per_page == 30)
        .return_once(|_| Ok(SearchResults { hits: vec![], total: 0 }));

    let uc = build_uc(svc);
    uc.search("rust".to_string(), vec![], 1, 100).await.expect("capped per_page");
}

#[tokio::test]
async fn per_page_below_30_is_passed_unchanged() {
    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(|q| q.per_page == 10)
        .return_once(|_| Ok(SearchResults { hits: vec![], total: 0 }));

    let uc = build_uc(svc);
    uc.search("rust".to_string(), vec![], 1, 10).await.expect("small per_page unchanged");
}

// ─── passthrough ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn valid_query_forwards_category_filter_and_returns_results() {
    let hit = SearchHit {
        thread_id: ids::thread_a(),
        thread_slug: "my-thread".to_string(),
        title: "My Thread".to_string(),
        excerpt: Some("A great post".to_string()),
    };
    let expected_hit = hit.clone();

    let mut svc = MockSearchService::new();
    let cat_id = ids::category_a();
    svc.expect_search()
        .withf(move |q| {
            q.q == "rust" && q.category_ids.contains(&cat_id) && q.page == 2
        })
        .return_once(move |_| Ok(SearchResults { hits: vec![hit], total: 1 }));

    let uc = build_uc(svc);
    let result = uc
        .search("rust".to_string(), vec![ids::category_a()], 2, 20)
        .await
        .expect("search with filters");

    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].thread_id, expected_hit.thread_id);
}
