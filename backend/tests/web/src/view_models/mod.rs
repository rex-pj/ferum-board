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
