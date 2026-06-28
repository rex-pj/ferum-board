use ferum_web::handlers::pages::search::SearchQuery;
use uuid::Uuid;

#[test]
fn search_query_all_optional() {
    let q: SearchQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.q.is_none());
    assert!(q.page.is_none());
    assert!(q.category_id.is_none());
}

#[test]
fn search_query_with_keyword_and_page() {
    let q: SearchQuery =
        serde_json::from_str(r#"{"q":"rust async","page":2}"#).unwrap();
    assert_eq!(q.q.as_deref(), Some("rust async"));
    assert_eq!(q.page, Some(2));
}

#[test]
fn search_query_with_category_filter() {
    let cat_id = Uuid::new_v4();
    let json = format!(r#"{{"category_id":"{cat_id}"}}"#);
    let q: SearchQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(q.category_id, Some(cat_id));
}
