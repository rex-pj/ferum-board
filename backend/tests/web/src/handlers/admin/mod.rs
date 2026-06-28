mod api;
mod pages;

use ferum_web::handlers::admin::{parse_opt_uuid, PageQuery};
use uuid::Uuid;

// ─── parse_opt_uuid ───────────────────────────────────────────────────────────

#[test]
fn parse_opt_uuid_none_input_returns_none() {
    assert!(parse_opt_uuid(None).is_none());
}

#[test]
fn parse_opt_uuid_empty_string_returns_none() {
    assert!(parse_opt_uuid(Some("")).is_none());
}

#[test]
fn parse_opt_uuid_invalid_string_returns_none() {
    assert!(parse_opt_uuid(Some("not-a-uuid")).is_none());
}

#[test]
fn parse_opt_uuid_valid_uuid_returns_some() {
    let id = Uuid::new_v4();
    let parsed = parse_opt_uuid(Some(&id.to_string()));
    assert_eq!(parsed, Some(id));
}

#[test]
fn parse_opt_uuid_whitespace_string_returns_none() {
    assert!(parse_opt_uuid(Some("   ")).is_none());
}

// ─── PageQuery ────────────────────────────────────────────────────────────────

#[test]
fn page_query_all_optional_fields() {
    let q: PageQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
    assert!(q.q.is_none());
    assert!(q.status.is_none());
    assert!(q.sort_by.is_none());
    assert!(q.sort_dir.is_none());
    assert!(q.actor_id.is_none());
    assert!(q.target_type.is_none());
    assert!(q.category_id.is_none());
    assert!(q.author_id.is_none());
    assert!(q.date_from.is_none());
    assert!(q.date_to.is_none());
}

#[test]
fn page_query_pagination_fields_deserialize() {
    let q: PageQuery =
        serde_json::from_str(r#"{"page":2,"per_page":25,"q":"test"}"#).unwrap();
    assert_eq!(q.page, Some(2));
    assert_eq!(q.per_page, Some(25));
    assert_eq!(q.q.as_deref(), Some("test"));
}

#[test]
fn page_query_filter_fields_deserialize() {
    let q: PageQuery = serde_json::from_str(
        r#"{"status":"pending","sort_by":"created_at","sort_dir":"desc","target_type":"thread"}"#,
    )
    .unwrap();
    assert_eq!(q.status.as_deref(), Some("pending"));
    assert_eq!(q.sort_by.as_deref(), Some("created_at"));
    assert_eq!(q.sort_dir.as_deref(), Some("desc"));
    assert_eq!(q.target_type.as_deref(), Some("thread"));
}
