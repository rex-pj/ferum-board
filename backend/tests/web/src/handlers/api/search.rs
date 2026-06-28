use ferum_web::view_models::search::SearchQuery;
use uuid::Uuid;

#[test]
fn search_query_all_none_by_default() {
    let q: SearchQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.q.is_none());
    assert!(q.category_id.is_none());
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
}

#[test]
fn search_query_with_text_and_pagination() {
    let q: SearchQuery = serde_json::from_str(r#"{"q":"axum tutorial","page":2,"per_page":10}"#).unwrap();
    assert_eq!(q.q.as_deref(), Some("axum tutorial"));
    assert_eq!(q.page, Some(2));
    assert_eq!(q.per_page, Some(10));
}

#[test]
fn search_query_with_category_filter() {
    let cat_id = Uuid::new_v4();
    let json = format!(r#"{{"q":"test","category_id":"{cat_id}"}}"#);
    let q: SearchQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(q.category_id, Some(cat_id));
}

#[test]
fn search_query_invalid_category_uuid_fails() {
    let result: Result<SearchQuery, _> =
        serde_json::from_str(r#"{"category_id":"not-a-uuid"}"#);
    assert!(result.is_err());
}
