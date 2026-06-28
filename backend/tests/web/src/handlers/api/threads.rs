use ferum_web::view_models::thread::{MarkSolvedRequest, MoveThreadRequest, ThreadListQuery};

// ─── ThreadListQuery ──────────────────────────────────────────────────────────

#[test]
fn thread_list_query_all_defaults_to_none() {
    let q: ThreadListQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
    assert!(q.tag.is_none());
    assert!(q.sort.is_none());
}

#[test]
fn thread_list_query_with_tag_filter() {
    let q: ThreadListQuery = serde_json::from_str(r#"{"tag":"rust","sort":"hottest"}"#).unwrap();
    assert_eq!(q.tag.as_deref(), Some("rust"));
    assert_eq!(q.sort.as_deref(), Some("hottest"));
}

#[test]
fn thread_list_query_sort_variants_deserialize() {
    for sort in &["latest", "newest", "hottest", "unanswered", "solved"] {
        let json = format!(r#"{{"sort":"{sort}"}}"#);
        let q: ThreadListQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(q.sort.as_deref(), Some(*sort));
    }
}

#[test]
fn thread_list_query_pagination_fields() {
    let q: ThreadListQuery = serde_json::from_str(r#"{"page":3,"per_page":50}"#).unwrap();
    assert_eq!(q.page, Some(3));
    assert_eq!(q.per_page, Some(50));
}

// ─── MoveThreadRequest ────────────────────────────────────────────────────────

#[test]
fn move_thread_request_deserializes_uuid() {
    let req: MoveThreadRequest = serde_json::from_str(
        r#"{"category_id":"00000000-0000-0000-0000-000000000099"}"#,
    )
    .unwrap();
    assert_eq!(req.category_id.to_string(), "00000000-0000-0000-0000-000000000099");
}

#[test]
fn move_thread_request_invalid_uuid_fails() {
    let result: Result<MoveThreadRequest, _> =
        serde_json::from_str(r#"{"category_id":"not-a-uuid"}"#);
    assert!(result.is_err());
}

#[test]
fn move_thread_request_missing_category_id_fails() {
    let result: Result<MoveThreadRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── MarkSolvedRequest ────────────────────────────────────────────────────────

#[test]
fn mark_solved_request_deserializes_uuid() {
    let req: MarkSolvedRequest = serde_json::from_str(
        r#"{"best_answer_id":"00000000-0000-0000-0000-000000000042"}"#,
    )
    .unwrap();
    assert_eq!(req.best_answer_id.to_string(), "00000000-0000-0000-0000-000000000042");
}

#[test]
fn mark_solved_request_missing_field_fails() {
    let result: Result<MarkSolvedRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}
