use ferum_web::view_models::webhook::{CreateWebhookRequest, UpdateWebhookRequest};
use validator::Validate;

// ─── CreateWebhookRequest ─────────────────────────────────────────────────────

#[test]
fn create_webhook_minimal_valid() {
    let req: CreateWebhookRequest = serde_json::from_str(
        r#"{"url":"https://example.com/hook","events":["post.created"]}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
    assert!(req.secret.is_none());
}

#[test]
fn create_webhook_empty_url_fails() {
    let req: CreateWebhookRequest =
        serde_json::from_str(r#"{"url":"","events":["post.created"]}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_webhook_empty_events_deserializes() {
    // events has no min-length validation in the struct — application layer checks this
    let req: CreateWebhookRequest =
        serde_json::from_str(r#"{"url":"https://example.com/hook","events":[]}"#).unwrap();
    assert!(req.events.is_empty());
}

#[test]
fn create_webhook_with_secret_valid() {
    let req: CreateWebhookRequest = serde_json::from_str(
        r#"{"url":"https://example.com/hook","events":["post.created"],"secret":"my-secret"}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
    assert_eq!(req.secret.as_deref(), Some("my-secret"));
}

#[test]
fn create_webhook_url_too_long_fails() {
    let long_url = format!("https://example.com/{}", "a".repeat(2048));
    let json = format!(r#"{{"url":"{long_url}","events":["post.created"]}}"#);
    let req: CreateWebhookRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

// ─── UpdateWebhookRequest ─────────────────────────────────────────────────────

#[test]
fn update_webhook_all_optional() {
    let req: UpdateWebhookRequest = serde_json::from_str(r#"{}"#).unwrap();
    assert!(req.url.is_none());
    assert!(req.events.is_none());
    assert!(req.is_active.is_none());
}

#[test]
fn update_webhook_deactivate() {
    let req: UpdateWebhookRequest =
        serde_json::from_str(r#"{"is_active":false}"#).unwrap();
    assert_eq!(req.is_active, Some(false));
}

#[test]
fn update_webhook_change_url_valid() {
    let req: UpdateWebhookRequest =
        serde_json::from_str(r#"{"url":"https://new-endpoint.com/hook"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn update_webhook_empty_events_deserializes() {
    // events has no min-length validation in the struct — application layer checks this
    let req: UpdateWebhookRequest =
        serde_json::from_str(r#"{"events":[]}"#).unwrap();
    assert!(req.events.as_ref().map(|e| e.is_empty()).unwrap_or(false));
}
