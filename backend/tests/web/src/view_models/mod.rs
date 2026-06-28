mod validators;

use ferum_web::view_models::{DataResponse, PagedResponse};

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
