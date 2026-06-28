use ferum_web::handlers::admin::pages::themes::ThemeFlash;

#[test]
fn theme_flash_defaults_to_all_none() {
    let flash = ThemeFlash::default();
    assert!(flash.success.is_none());
    assert!(flash.error.is_none());
}

#[test]
fn theme_flash_with_success_message() {
    let flash: ThemeFlash =
        serde_json::from_str(r#"{"success":"Theme activated successfully."}"#).unwrap();
    assert_eq!(flash.success.as_deref(), Some("Theme activated successfully."));
    assert!(flash.error.is_none());
}

#[test]
fn theme_flash_with_error_message() {
    let flash: ThemeFlash =
        serde_json::from_str(r#"{"error":"Failed to load theme."}"#).unwrap();
    assert!(flash.success.is_none());
    assert_eq!(flash.error.as_deref(), Some("Failed to load theme."));
}
