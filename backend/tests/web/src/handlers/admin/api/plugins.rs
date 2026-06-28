use ferum_web::view_models::plugin::{
    ConfigurePluginRequest, DebugHookRequest, PluginLogQueryParams, TogglePluginStatusRequest,
    UninstallPluginRequest,
};
use validator::Validate;

// ─── UninstallPluginRequest ───────────────────────────────────────────────────

#[test]
fn uninstall_with_valid_confirm_slug_passes() {
    let req: UninstallPluginRequest =
        serde_json::from_str(r#"{"confirm_slug":"my-plugin"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn uninstall_empty_confirm_slug_fails() {
    let req: UninstallPluginRequest =
        serde_json::from_str(r#"{"confirm_slug":""}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn uninstall_confirm_slug_too_long_fails() {
    let long_slug = "a".repeat(101);
    let json = format!(r#"{{"confirm_slug":"{long_slug}"}}"#);
    let req: UninstallPluginRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

// ─── PluginLogQueryParams ─────────────────────────────────────────────────────

#[test]
fn plugin_log_params_defaults_limit_to_100() {
    let params: PluginLogQueryParams = serde_json::from_str(r#"{}"#).unwrap();
    assert_eq!(params.limit, 100);
    assert_eq!(params.offset, 0);
    assert!(params.level.is_none());
    assert!(params.hook_name.is_none());
}

#[test]
fn plugin_log_params_with_filters_passes_validation() {
    let params: PluginLogQueryParams = serde_json::from_str(
        r#"{"level":"error","hook_name":"before_post_create","limit":50,"offset":10}"#,
    )
    .unwrap();
    assert!(params.validate().is_ok());
    assert_eq!(params.level.as_deref(), Some("error"));
    assert_eq!(params.limit, 50);
    assert_eq!(params.offset, 10);
}

#[test]
fn plugin_log_params_level_too_long_fails() {
    let long_level = "a".repeat(21);
    let json = format!(r#"{{"level":"{long_level}"}}"#);
    let params: PluginLogQueryParams = serde_json::from_str(&json).unwrap();
    assert!(params.validate().is_err());
}

#[test]
fn plugin_log_params_hook_name_too_long_fails() {
    let long_hook = "a".repeat(101);
    let json = format!(r#"{{"hook_name":"{long_hook}"}}"#);
    let params: PluginLogQueryParams = serde_json::from_str(&json).unwrap();
    assert!(params.validate().is_err());
}

// ─── DebugHookRequest ─────────────────────────────────────────────────────────

#[test]
fn debug_hook_valid_passes() {
    let req: DebugHookRequest = serde_json::from_str(
        r#"{"hook":"before_post_create","payload":{"content":"hello"}}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
    assert_eq!(req.hook, "before_post_create");
}

#[test]
fn debug_hook_empty_hook_name_fails() {
    let req: DebugHookRequest =
        serde_json::from_str(r#"{"hook":"","payload":{}}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn debug_hook_name_too_long_fails() {
    let long_hook = "a".repeat(201);
    let json = format!(r#"{{"hook":"{long_hook}","payload":{{}}}}"#);
    let req: DebugHookRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn debug_hook_missing_payload_fails() {
    let result: Result<DebugHookRequest, _> =
        serde_json::from_str(r#"{"hook":"some-hook"}"#);
    assert!(result.is_err());
}

// ─── ConfigurePluginRequest ───────────────────────────────────────────────────

#[test]
fn configure_plugin_with_json_config_deserializes() {
    let req: ConfigurePluginRequest = serde_json::from_str(
        r#"{"config":{"api_key":"abc123","enabled":true}}"#,
    )
    .unwrap();
    assert!(req.config.is_object());
}

// ─── TogglePluginStatusRequest ────────────────────────────────────────────────

#[test]
fn toggle_plugin_active_true_deserializes() {
    let req: TogglePluginStatusRequest =
        serde_json::from_str(r#"{"active":true}"#).unwrap();
    assert!(req.active);
}

#[test]
fn toggle_plugin_active_false_deserializes() {
    let req: TogglePluginStatusRequest =
        serde_json::from_str(r#"{"active":false}"#).unwrap();
    assert!(!req.active);
}
