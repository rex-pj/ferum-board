use ferum_web::view_models::report::{
    CreateReportRequest, ResolveReportRequest, TempBanRequest, WarnUserRequest,
};
use validator::Validate;

// ─── CreateReportRequest ──────────────────────────────────────────────────────

#[test]
fn create_report_valid_with_post_id() {
    let req: CreateReportRequest = serde_json::from_str(
        r#"{"post_id":"00000000-0000-0000-0000-000000000001","reason":"Spam content"}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn create_report_empty_reason_fails() {
    let req: CreateReportRequest = serde_json::from_str(
        r#"{"post_id":"00000000-0000-0000-0000-000000000001","reason":""}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_report_reason_too_long_fails() {
    let long_reason = "a".repeat(2001);
    let json = format!(
        r#"{{"post_id":"00000000-0000-0000-0000-000000000001","reason":"{long_reason}"}}"#
    );
    let req: CreateReportRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_report_no_ids_still_deserializes() {
    // Both post_id and thread_id are optional in the struct;
    // the at-least-one-required constraint is enforced at the application layer.
    let req: CreateReportRequest =
        serde_json::from_str(r#"{"reason":"Bad content"}"#).unwrap();
    assert!(req.post_id.is_none());
    assert!(req.thread_id.is_none());
}

#[test]
fn create_report_with_thread_id() {
    let req: CreateReportRequest = serde_json::from_str(
        r#"{"thread_id":"00000000-0000-0000-0000-000000000002","reason":"Off topic"}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
}

// ─── ResolveReportRequest ─────────────────────────────────────────────────────

#[test]
fn resolve_report_resolved_status_passes() {
    let req: ResolveReportRequest =
        serde_json::from_str(r#"{"status":"resolved"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn resolve_report_dismissed_status_passes() {
    let req: ResolveReportRequest =
        serde_json::from_str(r#"{"status":"dismissed"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn resolve_report_invalid_status_fails() {
    let req: ResolveReportRequest =
        serde_json::from_str(r#"{"status":"pending"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn resolve_report_empty_status_fails() {
    let req: ResolveReportRequest =
        serde_json::from_str(r#"{"status":""}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn resolve_report_with_moderator_notes_passes() {
    let req: ResolveReportRequest =
        serde_json::from_str(r#"{"status":"resolved","moderator_notes":"Handled."}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn resolve_report_moderator_notes_too_long_fails() {
    let long_notes = "a".repeat(2001);
    let json = format!(r#"{{"status":"resolved","moderator_notes":"{long_notes}"}}"#);
    let req: ResolveReportRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

// ─── WarnUserRequest ──────────────────────────────────────────────────────────

#[test]
fn valid_warn_user_passes() {
    let req: WarnUserRequest =
        serde_json::from_str(r#"{"reason":"Repeated off-topic posts"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn warn_user_empty_reason_fails() {
    let req: WarnUserRequest = serde_json::from_str(r#"{"reason":""}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn warn_user_reason_too_long_fails() {
    let long_reason = "a".repeat(1001);
    let json = format!(r#"{{"reason":"{long_reason}"}}"#);
    let req: WarnUserRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

// ─── TempBanRequest ───────────────────────────────────────────────────────────

#[test]
fn valid_temp_ban_passes() {
    let req: TempBanRequest = serde_json::from_str(
        r#"{"reason":"Spamming","until":"2099-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn temp_ban_empty_reason_fails() {
    let req: TempBanRequest = serde_json::from_str(
        r#"{"reason":"","until":"2099-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn temp_ban_reason_too_long_fails() {
    let long_reason = "a".repeat(1001);
    let json = format!(r#"{{"reason":"{long_reason}","until":"2099-01-01T00:00:00Z"}}"#);
    let req: TempBanRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn temp_ban_invalid_datetime_fails_to_deserialize() {
    let result: Result<TempBanRequest, _> =
        serde_json::from_str(r#"{"reason":"Spam","until":"not-a-date"}"#);
    assert!(result.is_err());
}
