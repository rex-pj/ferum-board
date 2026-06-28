use ferum_web::handlers::moderation::api::queue::PendingQueueQuery;
use uuid::Uuid;

#[test]
fn pending_queue_query_all_optional() {
    let q: PendingQueueQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.category_id.is_none());
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
}

#[test]
fn pending_queue_query_with_category_filter() {
    let cat_id = Uuid::new_v4();
    let json = format!(r#"{{"category_id":"{cat_id}","page":1,"per_page":20}}"#);
    let q: PendingQueueQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(q.category_id, Some(cat_id));
    assert_eq!(q.page, Some(1));
}

#[test]
fn pending_queue_query_invalid_uuid_fails() {
    let result: Result<PendingQueueQuery, _> =
        serde_json::from_str(r#"{"category_id":"bad-uuid"}"#);
    assert!(result.is_err());
}
