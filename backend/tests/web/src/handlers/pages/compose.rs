use ferum_web::handlers::pages::compose::NewThreadQuery;

#[test]
fn new_thread_query_no_category_preselect() {
    let q: NewThreadQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.category_id.is_none());
}

#[test]
fn new_thread_query_with_category_preselect() {
    let q: NewThreadQuery =
        serde_json::from_str(r#"{"category_id":"general"}"#).unwrap();
    assert_eq!(q.category_id.as_deref(), Some("general"));
}
