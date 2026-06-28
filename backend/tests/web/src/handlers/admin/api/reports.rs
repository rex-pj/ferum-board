use ferum_web::handlers::admin::api::reports::AuditLogQuery;
use uuid::Uuid;

// ─── AuditLogQuery ────────────────────────────────────────────────────────────

#[test]
fn audit_log_query_all_optional() {
    let q: AuditLogQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
    assert!(q.actor_id.is_none());
    assert!(q.target_type.is_none());
}

#[test]
fn audit_log_query_with_pagination() {
    let q: AuditLogQuery =
        serde_json::from_str(r#"{"page":1,"per_page":30}"#).unwrap();
    assert_eq!(q.page, Some(1));
    assert_eq!(q.per_page, Some(30));
}

#[test]
fn audit_log_query_with_actor_and_target_type() {
    let actor_id = Uuid::new_v4();
    let json = format!(r#"{{"actor_id":"{actor_id}","target_type":"thread"}}"#);
    let q: AuditLogQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(q.actor_id, Some(actor_id));
    assert_eq!(q.target_type.as_deref(), Some("thread"));
}

#[test]
fn audit_log_query_invalid_actor_uuid_fails() {
    let result: Result<AuditLogQuery, _> =
        serde_json::from_str(r#"{"actor_id":"not-a-uuid"}"#);
    assert!(result.is_err());
}
