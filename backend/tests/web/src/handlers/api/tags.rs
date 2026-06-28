use ferum_web::handlers::api::tags::CreateTagRequest;
use ferum_web::view_models::tag::TagListQuery;

// ─── CreateTagRequest ─────────────────────────────────────────────────────────

#[test]
fn create_tag_with_name_deserializes() {
    let req: CreateTagRequest = serde_json::from_str(r#"{"name":"Rust"}"#).unwrap();
    assert_eq!(req.name, "Rust");
}

#[test]
fn create_tag_missing_name_fails_to_deserialize() {
    let result: Result<CreateTagRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── TagListQuery ─────────────────────────────────────────────────────────────

#[test]
fn tag_list_query_empty_has_no_filter() {
    let q: TagListQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.q.is_none());
}

#[test]
fn tag_list_query_with_search_term() {
    let q: TagListQuery = serde_json::from_str(r#"{"q":"rust"}"#).unwrap();
    assert_eq!(q.q.as_deref(), Some("rust"));
}
