use std::sync::Arc;

use ferum_application::ports::{
    ProductFacets, ProductSearchSort, SearchKind, SearchResults, ThreadSearchSort,
};
use ferum_application::usecases::search_usecase::{SearchRequest, SearchScope, SearchUseCase};
use ferum_domain::models::category::ViewPolicy;
use ferum_domain::models::product::{ProductStatus, ProductType};
use ferum_domain::repositories::product_repository::ProductListItem;
use ferum_test_support::fixtures::{ids, make_category, make_product, AuthUserBuilder};
use ferum_test_support::mocks::category_repository::MockCategoryRepository;
use ferum_test_support::mocks::product_repository::MockProductRepository;
use ferum_test_support::mocks::search_service::MockSearchService;
use ferum_test_support::mocks::thread_repository::MockThreadRepository;
use uuid::Uuid;

fn empty() -> SearchResults {
    SearchResults { hits: vec![], total: 0 }
}

/// A non-zero total with no rows. Used by the tests that are about *which
/// parameters reach the backend*: a zero total makes the use case fire its
/// zero-result relaxation probes, which are extra calls those tests would then
/// have to model for no benefit. `hits` stays empty so hydration is skipped and
/// the repository mocks are never touched.
fn found(total: u64) -> SearchResults {
    SearchResults { hits: vec![], total }
}

/// Mocks with no expectations panic when touched — which is exactly the
/// assertion for paths that must not reach a repository.
fn build_uc(
    search: impl ferum_application::ports::SearchService + 'static,
    categories: MockCategoryRepository,
    products: MockProductRepository,
) -> SearchUseCase {
    SearchUseCase::new(
        Arc::new(search),
        Arc::new(MockThreadRepository::new()),
        Arc::new(categories),
        Arc::new(products),
    )
}

/// A plain request; tests override the one field they are about with `..`.
fn req(q: &str, scope: SearchScope) -> SearchRequest {
    SearchRequest {
        q: q.to_string(),
        scope,
        ..SearchRequest::default()
    }
}

/// A category repository returning one public category, which is what most of
/// these tests need to get past the visibility gate.
fn one_public_category() -> MockCategoryRepository {
    let mut cats = MockCategoryRepository::new();
    cats.expect_list_all()
        .returning(|| Ok(vec![make_category(ids::category_a())]));
    cats
}

// ─── empty query guard ────────────────────────────────────────────────────────

#[tokio::test]
async fn empty_query_returns_empty_results_without_calling_service() {
    let uc = build_uc(
        MockSearchService::new(),
        MockCategoryRepository::new(),
        MockProductRepository::new(),
    );

    let outcome = uc
        .search_all(None, req("", SearchScope::All))
        .await
        .expect("empty query succeeds");
    assert!(outcome.is_empty());
    assert!(outcome.threads.is_empty());
    assert!(outcome.products.is_empty());
}

#[tokio::test]
async fn whitespace_only_query_returns_empty_results() {
    let uc = build_uc(
        MockSearchService::new(),
        MockCategoryRepository::new(),
        MockProductRepository::new(),
    );

    let outcome = uc
        .search_all(None, req("   ", SearchScope::All))
        .await
        .expect("whitespace query succeeds");
    assert!(outcome.is_empty());
}

// ─── per_page cap ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn per_page_is_capped_at_30() {
    let mut svc = MockSearchService::new();
    // The product leg of an `All` search uses a fixed preview size, so only the
    // thread leg carries the caller's per_page.
    svc.expect_search()
        .withf(|q| q.kind != SearchKind::Thread || q.per_page == 30)
        .times(2)
        .returning(|_| Ok(empty()));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(None, SearchRequest { per_page: 100, ..req("rust", SearchScope::All) })
        .await
        .expect("capped per_page");
}

#[tokio::test]
async fn per_page_below_30_is_passed_unchanged() {
    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(|q| q.kind != SearchKind::Thread || q.per_page == 10)
        .times(2)
        .returning(|_| Ok(empty()));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(None, SearchRequest { per_page: 10, ..req("rust", SearchScope::All) })
        .await
        .expect("small per_page unchanged");
}

/// The requested discussion order reaches the thread leg of the search; the
/// product leg never reads it.
#[tokio::test]
async fn thread_sort_is_forwarded_to_the_thread_query() {
    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(|q| q.kind != SearchKind::Thread || q.thread_sort == ThreadSearchSort::MostReplies)
        .times(2)
        .returning(|_| Ok(empty()));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(
        None,
        SearchRequest {
            thread_sort: ThreadSearchSort::MostReplies,
            ..req("rust", SearchScope::All)
        },
    )
    .await
    .expect("thread sort forwarded");
}

// ─── category visibility ──────────────────────────────────────────────────────

/// The use case resolves visibility itself; a `staff_only` category must never
/// reach the search backend as an allowed id, or its thread titles leak to
/// guests through search.
#[tokio::test]
async fn staff_only_categories_are_excluded_for_guests() {
    let public_id = ids::category_a();
    let staff_id = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();

    let mut cats = MockCategoryRepository::new();
    cats.expect_list_all().returning(move || {
        let public = make_category(public_id);
        let mut staff = make_category(staff_id);
        staff.id = staff_id;
        staff.slug = "staff".to_string();
        staff.view_policy = ViewPolicy::StaffOnly;
        Ok(vec![public, staff])
    });

    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(move |q| match q.kind {
            SearchKind::Thread => {
                q.visible_category_ids == vec![public_id]
                    && !q.visible_category_ids.contains(&staff_id)
            }
            SearchKind::Product => true,
        })
        .times(2)
        .returning(|_| Ok(empty()));

    let uc = build_uc(svc, cats, MockProductRepository::new());
    uc.search_all(None, req("rust", SearchScope::All))
        .await
        .expect("search");
}

/// Selecting a category narrows to it and its children, and the narrowing is
/// applied on top of visibility, never instead of it.
#[tokio::test]
async fn category_filter_narrows_to_the_selected_subtree() {
    let selected = ids::category_a();
    let other = Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap();

    let mut cats = MockCategoryRepository::new();
    cats.expect_list_all().returning(move || {
        let mut b = make_category(other);
        b.slug = "other".to_string();
        Ok(vec![make_category(selected), b])
    });

    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(move |q| q.kind != SearchKind::Thread || q.visible_category_ids == vec![selected])
        .times(2)
        .returning(|_| Ok(found(1)));

    let uc = build_uc(svc, cats, MockProductRepository::new());
    uc.search_all(
        None,
        SearchRequest { category_id: Some(selected), ..req("rust", SearchScope::All) },
    )
    .await
    .expect("search");
}

// ─── scope routing ────────────────────────────────────────────────────────────

/// On the Products tab the discussion leg is reduced to a count: the tab badge
/// needs the number, the page does not need the rows.
#[tokio::test]
async fn products_scope_counts_threads_without_fetching_them() {
    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(|q| match q.kind {
            SearchKind::Thread => q.count_only,
            SearchKind::Product => !q.count_only && q.per_page == 20,
        })
        .times(2)
        .returning(|q| {
            Ok(SearchResults {
                hits: vec![],
                total: if q.kind == SearchKind::Thread { 7 } else { 0 },
            })
        });

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    let outcome = uc
        .search_all(None, req("rust", SearchScope::Products))
        .await
        .expect("search");

    assert_eq!(outcome.thread_total, 7, "the badge count still arrives");
    assert!(outcome.threads.is_empty(), "but no thread rows were hydrated");
}

/// The viewer is forwarded so the search backend can include their own pending
/// submissions — a contributor must be able to find what they just submitted.
#[tokio::test]
async fn viewer_id_is_forwarded_for_product_visibility() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(|q| q.viewer_id == Some(ids::user_a()))
        .times(2)
        .returning(|_| Ok(empty()));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(Some(&actor), req("oak", SearchScope::All))
    .await
    .expect("search");
}

// ─── product hydration ────────────────────────────────────────────────────────

#[tokio::test]
async fn product_hits_are_hydrated_with_their_rating() {
    let product_id = ids::product_a();

    let mut svc = MockSearchService::new();
    svc.expect_search().times(2).returning(move |q| match q.kind {
        SearchKind::Thread => Ok(empty()),
        SearchKind::Product => Ok(SearchResults {
            hits: vec![ferum_application::ports::SearchHit {
                kind: SearchKind::Product,
                id: product_id,
                slug: "test-product".to_string(),
                title: "Test product".to_string(),
                excerpt: None,
            }],
            total: 1,
        }),
    });

    let mut products = MockProductRepository::new();
    products.expect_find_many_by_ids().returning(move |_| {
        Ok(vec![ProductListItem {
            product: make_product(product_id, None, ProductStatus::Published),
            review_count: 34,
            avg_overall: Some(rust_decimal::Decimal::new(46, 1)),
        }])
    });

    let uc = build_uc(svc, one_public_category(), products);
    let outcome = uc
        .search_all(None, req("test", SearchScope::All))
        .await
        .expect("search");

    assert_eq!(outcome.product_total, 1);
    assert_eq!(outcome.products.len(), 1);
    assert_eq!(outcome.products[0].review_count, 34);
    assert_eq!(outcome.products[0].avg_overall, Some(4.6));
}

/// A product deleted after it was indexed leaves a hit with nothing behind it.
/// A product card is entirely metadata, so an unresolvable one is dropped
/// rather than rendered blank.
#[tokio::test]
async fn product_hits_that_no_longer_resolve_are_dropped() {
    let mut svc = MockSearchService::new();
    svc.expect_search().times(2).returning(|q| match q.kind {
        SearchKind::Thread => Ok(empty()),
        SearchKind::Product => Ok(SearchResults {
            hits: vec![ferum_application::ports::SearchHit {
                kind: SearchKind::Product,
                id: ids::product_a(),
                slug: "gone".to_string(),
                title: "Gone".to_string(),
                excerpt: None,
            }],
            total: 1,
        }),
    });

    let mut products = MockProductRepository::new();
    products.expect_find_many_by_ids().returning(|_| Ok(vec![]));

    let uc = build_uc(svc, one_public_category(), products);
    let outcome = uc
        .search_all(None, req("gone", SearchScope::All))
        .await
        .expect("search");

    assert!(outcome.products.is_empty());
    // The total is what the engine reported; it is not re-derived from the
    // hydrated rows, which would make the count drift per page.
    assert_eq!(outcome.product_total, 1);
}

// ─── product facets ───────────────────────────────────────────────────────────

/// Facets belong to the product leg only. Handing them to the thread search
/// would be harmless today (the adapters ignore them) and a live bug the moment
/// a thread facet is added, so the split is asserted rather than assumed.
#[tokio::test]
async fn facets_reach_the_product_leg_and_not_the_thread_leg() {
    let brand = Uuid::parse_str("00000000-0000-0000-0000-0000000000b1").unwrap();
    let material = Uuid::parse_str("00000000-0000-0000-0000-0000000000c1").unwrap();

    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(move |q| match q.kind {
            SearchKind::Product => {
                q.facets.product_type == Some(ProductType::Furniture)
                    && q.facets.brand_id == Some(brand)
                    && q.facets.material_id == Some(material)
                    && q.facets.sort == ProductSearchSort::TopRated
            }
            SearchKind::Thread => {
                q.facets.brand_id.is_none()
                    && q.facets.material_id.is_none()
                    && q.facets.product_type.is_none()
            }
        })
        .times(2)
        .returning(|_| Ok(found(1)));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(
        None,
        SearchRequest {
            facets: ProductFacets {
                product_type: Some(ProductType::Furniture),
                brand_id: Some(brand),
                material_id: Some(material),
                sort: ProductSearchSort::TopRated,
            },
            ..req("sofa", SearchScope::Products)
        },
    )
    .await
    .expect("search");
}

/// Sorting is not filtering. A page that claims "filters active" because the
/// reader chose an ordering misreports its own state.
#[test]
fn sort_alone_does_not_count_as_a_narrowed_facet() {
    let sorted = ProductFacets {
        sort: ProductSearchSort::TopRated,
        ..ProductFacets::default()
    };
    assert!(!sorted.is_narrowed());

    let filtered = ProductFacets {
        product_type: Some(ProductType::Furniture),
        ..ProductFacets::default()
    };
    assert!(filtered.is_narrowed());
}

/// An unknown `?psort=` must degrade to relevance, not error — stale links and
/// hand-edited URLs are normal traffic.
#[test]
fn unknown_sort_falls_back_to_relevance() {
    assert_eq!(ProductSearchSort::parse("nonsense"), ProductSearchSort::Relevance);
    assert_eq!(ProductSearchSort::parse(""), ProductSearchSort::Relevance);
    assert_eq!(ProductSearchSort::parse("top_rated"), ProductSearchSort::TopRated);
    // Round-trips, so the value a link carries is the value the page reads back.
    for s in [
        ProductSearchSort::Relevance,
        ProductSearchSort::TopRated,
        ProductSearchSort::MostReviewed,
        ProductSearchSort::Newest,
    ] {
        assert_eq!(ProductSearchSort::parse(s.as_str()), s);
    }
}

// ─── the two taxonomies stay apart ────────────────────────────────────────────

/// A *forum* category narrows discussions and nothing else. Forum categories
/// organise conversation — Off-Topic, Programming, Site Feedback — and none of
/// them is a sensible home for a sofa, so leaking one onto the product leg
/// would filter the catalogue by a tree it was never filed in.
#[tokio::test]
async fn a_forum_category_narrows_threads_only() {
    let selected = ids::category_a();

    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(move |q| match q.kind {
            SearchKind::Thread => q.visible_category_ids == vec![selected],
            SearchKind::Product => q.in_category_ids.is_empty(),
        })
        .times(2)
        .returning(|_| Ok(found(1)));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(
        None,
        SearchRequest { category_id: Some(selected), ..req("sofa", SearchScope::All) },
    )
    .await
    .expect("search");
}

/// And the mirror: a *catalogue* category narrows products and nothing else.
#[tokio::test]
async fn a_catalogue_category_narrows_products_only() {
    let pcat = Uuid::parse_str("00000000-0000-0000-0000-0000000000d1").unwrap();

    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(move |q| match q.kind {
            SearchKind::Product => q.in_category_ids == vec![pcat],
            // The thread leg keeps its full visible set — a catalogue category
            // must not shrink the discussion count.
            SearchKind::Thread => q.visible_category_ids == vec![ids::category_a()],
        })
        .times(2)
        .returning(|_| Ok(found(1)));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(
        None,
        SearchRequest {
            product_category_ids: vec![pcat],
            ..req("sofa", SearchScope::All)
        },
    )
    .await
    .expect("search");
}

/// With no category chosen, products must not be narrowed at all — the empty
/// list has to mean "everything" on the product side, the opposite of what it
/// means for the visibility set.
#[tokio::test]
async fn no_chosen_category_leaves_products_unnarrowed() {
    let mut svc = MockSearchService::new();
    svc.expect_search()
        .withf(|q| q.kind != SearchKind::Product || q.in_category_ids.is_empty())
        .times(2)
        .returning(|_| Ok(empty()));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    uc.search_all(None, req("sofa", SearchScope::All))
        .await
        .expect("search");
}

// ─── zero-result relaxation ───────────────────────────────────────────────────

/// On an empty intersection the use case works out what dropping each filter
/// would restore, so the page can name the way out instead of shrugging.
#[tokio::test]
async fn empty_results_report_what_dropping_each_filter_would_yield() {
    let pcat = Uuid::parse_str("00000000-0000-0000-0000-0000000000d1").unwrap();

    let mut svc = MockSearchService::new();
    svc.expect_search().returning(move |q| {
        let narrowed_by_category = !q.in_category_ids.is_empty();
        let narrowed_by_facets = q.facets.is_narrowed();
        let total = match (q.kind, narrowed_by_category, narrowed_by_facets) {
            // The actual search: both filters on, nothing matches.
            (SearchKind::Product, true, true) => 0,
            // Probe with the facets dropped, category kept.
            (SearchKind::Product, true, false) => 8,
            // Probe with the category dropped, facets kept.
            (SearchKind::Product, false, true) => 40,
            _ => 0,
        };
        Ok(SearchResults { hits: vec![], total })
    });

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    let outcome = uc
        .search_all(
            None,
            SearchRequest {
                // On the Products tab "the category" is the catalogue one —
                // that is the filter the reader can see narrowing the page.
                product_category_ids: vec![pcat],
                facets: ProductFacets {
                    brand_id: Some(Uuid::new_v4()),
                    ..ProductFacets::default()
                },
                ..req("sofa", SearchScope::Products)
            },
        )
        .await
        .expect("search");

    assert_eq!(outcome.product_total, 0);
    assert_eq!(outcome.relaxed.without_facets, Some(8));
    assert_eq!(outcome.relaxed.without_category, Some(40));
    assert!(outcome.relaxed.has_any());
}

/// No probes when there is nothing to relax — a query that simply found nothing
/// must not pay for extra counts, and must not offer a filter to drop that the
/// reader never set.
#[tokio::test]
async fn no_relaxation_probes_when_no_filter_is_set() {
    let mut svc = MockSearchService::new();
    // Exactly the two legs of the search itself; a third call means a probe ran.
    svc.expect_search().times(2).returning(|_| Ok(empty()));

    let uc = build_uc(svc, one_public_category(), MockProductRepository::new());
    let outcome = uc
        .search_all(None, req("nothing-matches-this", SearchScope::Products))
        .await
        .expect("search");

    assert_eq!(outcome.relaxed.without_category, None);
    assert_eq!(outcome.relaxed.without_facets, None);
    assert!(!outcome.relaxed.has_any());
}

/// A relaxation that still yields zero is not a way out. `has_any` is what the
/// page keys the whole recovery block off, so it must not fire on a zero.
#[test]
fn relaxation_yielding_zero_is_not_offered() {
    let none_left = ferum_application::usecases::search_usecase::RelaxedCounts {
        without_category: Some(0),
        without_facets: None,
    };
    assert!(!none_left.has_any());

    let some_left = ferum_application::usecases::search_usecase::RelaxedCounts {
        without_category: Some(3),
        without_facets: None,
    };
    assert!(some_left.has_any());
}
