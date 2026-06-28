use ferum_web::view_models::setup::RunSetupRequest;
use validator::Validate;

fn valid_setup() -> RunSetupRequest {
    serde_json::from_str(
        r#"{"admin_username":"admin","admin_email":"admin@example.com","admin_password":"hunter2!"}"#,
    )
    .unwrap()
}

#[test]
fn valid_run_setup_passes() {
    assert!(valid_setup().validate().is_ok());
}

#[test]
fn setup_admin_username_too_short_fails() {
    let req: RunSetupRequest = serde_json::from_str(
        r#"{"admin_username":"ab","admin_email":"admin@example.com","admin_password":"hunter2!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn setup_admin_username_too_long_fails() {
    let long_name = "a".repeat(31);
    let json = format!(
        r#"{{"admin_username":"{long_name}","admin_email":"a@b.com","admin_password":"hunter2!"}}"#
    );
    let req: RunSetupRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn setup_invalid_email_fails() {
    let req: RunSetupRequest = serde_json::from_str(
        r#"{"admin_username":"admin","admin_email":"notanemail","admin_password":"hunter2!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn setup_weak_password_fails() {
    let req: RunSetupRequest = serde_json::from_str(
        r#"{"admin_username":"admin","admin_email":"a@b.com","admin_password":"nospecial1"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn setup_seed_example_data_defaults_to_false() {
    let req = valid_setup();
    assert!(!req.seed_example_data);
}

#[test]
fn setup_with_nested_config_passes() {
    let req: RunSetupRequest = serde_json::from_str(
        r##"{"admin_username":"admin","admin_email":"a@b.com","admin_password":"hunter2!",
            "config":{"site_name":"My Forum","primary_color":"#1a2b3c"}}"##,
    )
    .unwrap();
    assert!(req.validate().is_ok());
    let config = req.config.unwrap();
    assert_eq!(config.site_name.as_deref(), Some("My Forum"));
}

#[test]
fn setup_config_site_name_too_long_fails() {
    let long_name = "a".repeat(101);
    let json = format!(
        r#"{{"admin_username":"admin","admin_email":"a@b.com","admin_password":"hunter2!","config":{{"site_name":"{long_name}"}}}}"#
    );
    let req: RunSetupRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}
