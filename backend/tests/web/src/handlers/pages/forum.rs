use ferum_web::handlers::pages::forum::ListQuery;

#[test]
fn list_query_all_optional() {
    let q: ListQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
    assert!(q.tag.is_none());
    assert!(q.sort.is_none());
}

#[test]
fn list_query_with_pagination_and_sort() {
    let q: ListQuery =
        serde_json::from_str(r#"{"page":2,"per_page":30,"sort":"hottest"}"#).unwrap();
    assert_eq!(q.page, Some(2));
    assert_eq!(q.per_page, Some(30));
    assert_eq!(q.sort.as_deref(), Some("hottest"));
}

#[test]
fn list_query_with_tag_filter() {
    let q: ListQuery = serde_json::from_str(r#"{"tag":"rust-lang"}"#).unwrap();
    assert_eq!(q.tag.as_deref(), Some("rust-lang"));
}
