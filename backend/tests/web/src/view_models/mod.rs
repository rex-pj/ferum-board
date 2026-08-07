mod validators;

use ferum_application::constants::MAX_PAGE;
use ferum_web::view_models::page_context::PaginationCtx;
use ferum_web::view_models::{DataResponse, PagedResponse};

// ─── PaginationCtx arithmetic ─────────────────────────────────────────────────

/// `?per_page=0` used to underflow `total + per_page - 1` on an empty result set:
/// a panic in debug, `u64::MAX` in release. Handlers clamp their query params, but
/// the constructor is the backstop for all 16 call sites.
#[test]
fn pagination_survives_zero_per_page_with_no_rows() {
    let p = PaginationCtx::simple(1, 0, 0);
    assert_eq!(p.total_pages, 0);
    assert!(!p.has_next);
    assert!(!p.has_prev);
}

#[test]
fn pagination_zero_per_page_does_not_report_absurd_page_count() {
    let p = PaginationCtx::simple(1, 0, 42);
    assert_eq!(p.total_pages, 42, "a zero per_page is treated as 1 per page");
}

#[test]
fn pagination_rounds_partial_last_page_up() {
    let p = PaginationCtx::simple(1, 20, 41);
    assert_eq!(p.total_pages, 3);
    assert!(p.has_next);
}

// `blank_as_none_uuid` is exercised through the real `SearchQuery` that uses it,
// in `handlers::api::search`.

// ─── DataResponse JSON shape ──────────────────────────────────────────────────

#[test]
fn data_response_serializes_with_data_key() {
    let resp = DataResponse::new(42u32);
    let json = serde_json::to_string(&resp).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["data"], 42);
}

#[test]
fn data_response_wraps_string_value() {
    let resp = DataResponse::new("hello");
    let json = serde_json::to_string(&resp).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["data"], "hello");
}

// ─── PagedResponse JSON shape ─────────────────────────────────────────────────

#[test]
fn paged_response_serializes_data_and_meta() {
    let resp = PagedResponse::new(vec![1u32, 2, 3], 100, 1, 20);
    let json = serde_json::to_string(&resp).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["data"], serde_json::json!([1, 2, 3]));
    assert_eq!(v["meta"]["total"], 100);
    assert_eq!(v["meta"]["page"], 1);
    assert_eq!(v["meta"]["per_page"], 20);
}

#[test]
fn paged_response_with_empty_data() {
    let resp = PagedResponse::<u32>::new(vec![], 0, 1, 20);
    let json = serde_json::to_string(&resp).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["data"], serde_json::json!([]));
    assert_eq!(v["meta"]["total"], 0);
}

#[test]
fn paged_response_meta_has_all_three_fields() {
    let resp = PagedResponse::new(vec!["a", "b"], 50, 3, 10);
    let json = serde_json::to_string(&resp).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let meta = &v["meta"];
    assert!(meta.get("total").is_some());
    assert!(meta.get("page").is_some());
    assert!(meta.get("per_page").is_some());
}

// ─── PaginationCtx respects the reachable-page ceiling ───────────────────────
//
// `utils::paginate` rejects `?page=` past MAX_PAGE, so the navigation must not
// advertise pages beyond it. These guard the seam between the two: they only
// diverge once a forum holds enough rows to exceed the ceiling, which is
// exactly when nobody is watching.

#[test]
fn total_pages_is_capped_at_the_reachable_ceiling() {
    // 50k threads at 20/page is 2,500 pages of data but only MAX_PAGE reachable.
    let p = PaginationCtx::simple(1, 20, 50_000);
    assert_eq!(
        p.total_pages, MAX_PAGE,
        "navigation must not offer a page the server refuses"
    );
}

#[test]
fn the_reported_total_is_not_capped() {
    // Only reachability is bounded — "50,000 threads" is still the truth, and
    // capping it would make the page lie about how much content exists.
    let p = PaginationCtx::simple(1, 20, 50_000);
    assert_eq!(p.total, 50_000);
}

#[test]
fn has_next_is_false_on_the_last_reachable_page() {
    let p = PaginationCtx::simple(MAX_PAGE, 20, 50_000);
    assert!(
        !p.has_next,
        "a `next` link from the final reachable page would 422"
    );
    assert!(p.has_prev);
}

#[test]
fn small_result_sets_are_unaffected() {
    let p = PaginationCtx::simple(1, 20, 100);
    assert_eq!(p.total_pages, 5);
    assert!(p.has_next);
}

// ─── Feed control URLs ────────────────────────────────────────────────────────
//
// The listing has two independent axes — ORDER (sort tabs) and NARROWING (filter
// chips) — and every link has to carry the axis the reader did NOT just touch.
// Getting that wrong produces a page that looks entirely correct and silently
// discards the reader's filter the moment they reorder, so it is pinned here
// rather than trusted to a template.

mod feed_controls {
    use ferum_domain::repositories::thread_repository::{
        parse_feed_query, ThreadFeedFilter, ThreadSort,
    };
    use ferum_web::view_models::page_context::{feed_query_tail, feed_url, FeedControlsCtx};

    #[test]
    fn the_default_view_has_no_query_string() {
        assert_eq!(
            feed_url("/", ThreadSort::Activity, ThreadFeedFilter::All, None),
            "/",
            "the default listing must not live at a second URL rendering the same rows"
        );
        assert_eq!(feed_query_tail(ThreadSort::Activity, ThreadFeedFilter::All, None), "");
    }

    #[test]
    fn each_axis_appears_only_when_it_is_not_the_default() {
        assert_eq!(
            feed_url("/", ThreadSort::Newest, ThreadFeedFilter::All, None),
            "/?sort=newest"
        );
        assert_eq!(
            feed_url("/", ThreadSort::Activity, ThreadFeedFilter::Solved, None),
            "/?filter=solved"
        );
        assert_eq!(
            feed_url("/", ThreadSort::MostReplies, ThreadFeedFilter::Unanswered, None),
            "/?sort=most_replies&filter=unanswered"
        );
    }

    #[test]
    fn pagination_tail_keeps_its_leading_ampersand_for_every_part() {
        // Callers append this after an existing `?page=N`.
        let tail = feed_query_tail(ThreadSort::Newest, ThreadFeedFilter::Solved, Some("rust"));
        assert_eq!(tail, "&tag=rust&sort=newest&filter=solved");
        assert!(tail.starts_with('&'));
    }

    /// Changing the ordering must not silently drop the narrowing the reader
    /// asked for — the failure that made the pre-split, single-parameter design
    /// unusable.
    #[test]
    fn sort_tabs_carry_the_active_filter_through() {
        let controls =
            FeedControlsCtx::build("/", ThreadSort::Activity, ThreadFeedFilter::Unanswered);
        for tab in &controls.sorts {
            assert!(
                tab.href.contains("filter=unanswered"),
                "sort tab {} dropped the active filter: {}",
                tab.key,
                tab.href
            );
        }
    }

    /// And the reverse: toggling a chip must not reset the ordering.
    #[test]
    fn filter_chips_carry_the_active_sort_through() {
        let controls =
            FeedControlsCtx::build("/", ThreadSort::MostReplies, ThreadFeedFilter::All);
        for chip in &controls.filters {
            assert!(
                chip.href.contains("sort=most_replies"),
                "filter chip {} dropped the active sort: {}",
                chip.key,
                chip.href
            );
        }
    }

    /// A chip that is on links to the view WITHOUT it. That is the entire toggle
    /// mechanism — there is no JavaScript behind these.
    #[test]
    fn an_active_chip_links_to_the_view_without_it() {
        let controls =
            FeedControlsCtx::build("/", ThreadSort::Activity, ThreadFeedFilter::Solved);

        let solved = controls.filters.iter().find(|c| c.key == "solved").unwrap();
        assert!(solved.active);
        assert_eq!(solved.href, "/", "pressing an active chip must clear it");

        let unanswered = controls
            .filters
            .iter()
            .find(|c| c.key == "unanswered")
            .unwrap();
        assert!(!unanswered.active);
        assert_eq!(
            unanswered.href, "/?filter=unanswered",
            "the two chips are mutually exclusive, so one replaces the other"
        );
    }

    #[test]
    fn exactly_one_sort_is_marked_active_and_it_is_the_current_one() {
        for sort in [ThreadSort::Activity, ThreadSort::Newest, ThreadSort::MostReplies] {
            let controls = FeedControlsCtx::build("/", sort, ThreadFeedFilter::All);
            let active: Vec<&str> = controls
                .sorts
                .iter()
                .filter(|t| t.active)
                .map(|t| t.key.as_str())
                .collect();
            assert_eq!(active, vec![sort.as_str()]);
        }
    }

    #[test]
    fn a_category_listing_builds_absolute_urls_under_its_own_slug() {
        let controls =
            FeedControlsCtx::build("/forum/help", ThreadSort::Activity, ThreadFeedFilter::All);
        assert_eq!(controls.sorts[0].href, "/forum/help");
        assert_eq!(controls.sorts[1].href, "/forum/help?sort=newest");
        assert_eq!(controls.filters[0].href, "/forum/help?filter=unanswered");
    }

    /// Every URL the UI emits must parse back to the state that produced it, and
    /// never be mistaken for a pre-split URL — otherwise ordinary navigation
    /// would bounce through a 301 on every click.
    #[test]
    fn every_emitted_href_parses_back_and_never_redirects() {
        for sort in [ThreadSort::Activity, ThreadSort::Newest, ThreadSort::MostReplies] {
            for filter in [
                ThreadFeedFilter::All,
                ThreadFeedFilter::Unanswered,
                ThreadFeedFilter::Solved,
            ] {
                let controls = FeedControlsCtx::build("/", sort, filter);
                for control in controls.sorts.iter().chain(controls.filters.iter()) {
                    let query = control.href.split_once('?').map(|(_, q)| q).unwrap_or("");
                    let mut sort_param = None;
                    let mut filter_param = None;
                    for pair in query.split('&').filter(|p| !p.is_empty()) {
                        match pair.split_once('=') {
                            Some(("sort", v)) => sort_param = Some(v),
                            Some(("filter", v)) => filter_param = Some(v),
                            _ => {}
                        }
                    }
                    let (_, _, legacy) = parse_feed_query(sort_param, filter_param);
                    assert!(
                        !legacy,
                        "href {} would send a normal click through a 301",
                        control.href
                    );
                }
            }
        }
    }
}
