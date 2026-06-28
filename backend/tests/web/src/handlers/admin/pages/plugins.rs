use ferum_web::handlers::admin::pages::plugins::{SavePluginConfigForm, UninstallPluginForm};

// ─── UninstallPluginForm ──────────────────────────────────────────────────────

#[test]
fn uninstall_plugin_form_deserializes() {
    let form: UninstallPluginForm =
        serde_json::from_str(r#"{"confirm_slug":"my-plugin"}"#).unwrap();
    assert_eq!(form.confirm_slug, "my-plugin");
}

#[test]
fn uninstall_plugin_form_missing_field_fails() {
    let result: Result<UninstallPluginForm, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── SavePluginConfigForm ─────────────────────────────────────────────────────

#[test]
fn save_plugin_config_form_deserializes() {
    let form: SavePluginConfigForm =
        serde_json::from_str(r#"{"config_json":"{\"key\":\"value\"}"}"#).unwrap();
    assert_eq!(form.config_json, r#"{"key":"value"}"#);
}

#[test]
fn save_plugin_config_form_empty_json_string() {
    let form: SavePluginConfigForm =
        serde_json::from_str(r#"{"config_json":"{}"}"#).unwrap();
    assert_eq!(form.config_json, "{}");
}

#[test]
fn save_plugin_config_form_missing_field_fails() {
    let result: Result<SavePluginConfigForm, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}
