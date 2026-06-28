use ferum_web::handlers::api::profile::UpdateProfileRequest;
use validator::Validate;

// ─── UpdateProfileRequest ─────────────────────────────────────────────────────

#[test]
fn valid_update_profile_all_none_passes() {
    let req: UpdateProfileRequest = serde_json::from_str(r#"{}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn valid_update_profile_with_all_fields_passes() {
    let req: UpdateProfileRequest = serde_json::from_str(
        r#"{"display_name":"Alice Smith","bio":"Rust dev","website":"https://alice.dev"}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn display_name_too_long_fails() {
    let long_name = "a".repeat(81);
    let json = format!(r#"{{"display_name":"{long_name}"}}"#);
    let req: UpdateProfileRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn bio_too_long_fails() {
    let long_bio = "a".repeat(501);
    let json = format!(r#"{{"bio":"{long_bio}"}}"#);
    let req: UpdateProfileRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn website_invalid_url_fails() {
    let req: UpdateProfileRequest =
        serde_json::from_str(r#"{"website":"not-a-url"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn website_valid_https_url_passes() {
    let req: UpdateProfileRequest =
        serde_json::from_str(r#"{"website":"https://example.com"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn website_too_long_fails() {
    let long_url = format!("https://example.com/{}", "a".repeat(185));
    let json = format!(r#"{{"website":"{long_url}"}}"#);
    let req: UpdateProfileRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

// ─── ChangePasswordRequest deserialization ───────────────────────────────────

#[test]
fn change_password_requires_both_fields() {
    use ferum_web::handlers::api::profile::ChangePasswordRequest;
    let result: Result<ChangePasswordRequest, _> =
        serde_json::from_str(r#"{"current_password":"old"}"#);
    assert!(result.is_err());
}

#[test]
fn change_password_both_fields_deserializes() {
    use ferum_web::handlers::api::profile::ChangePasswordRequest;
    let req: ChangePasswordRequest = serde_json::from_str(
        r#"{"current_password":"old_pass","new_password":"new_pass"}"#,
    )
    .unwrap();
    assert_eq!(req.current_password, "old_pass");
    assert_eq!(req.new_password, "new_pass");
}

// ─── UpdatePreferencesRequest deserialization ─────────────────────────────────

#[test]
fn update_preferences_all_none_deserializes() {
    use ferum_web::handlers::api::profile::UpdatePreferencesRequest;
    let req: UpdatePreferencesRequest = serde_json::from_str(r#"{}"#).unwrap();
    assert!(req.theme.is_none());
    assert!(req.font_size.is_none());
    assert!(req.layout.is_none());
}

#[test]
fn update_preferences_with_values_deserializes() {
    use ferum_web::handlers::api::profile::UpdatePreferencesRequest;
    let req: UpdatePreferencesRequest = serde_json::from_str(
        r#"{"theme":"dark","font_size":"large","layout":"compact"}"#,
    )
    .unwrap();
    assert_eq!(req.theme.as_deref(), Some("dark"));
    assert_eq!(req.font_size.as_deref(), Some("large"));
    assert_eq!(req.layout.as_deref(), Some("compact"));
}
