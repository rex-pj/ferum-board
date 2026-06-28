use ferum_web::view_models::auth::{ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest};
use validator::Validate;

// ─── RegisterRequest ──────────────────────────────────────────────────────────

fn valid_register() -> RegisterRequest {
    serde_json::from_str(
        r#"{"username":"alice","email":"alice@example.com","password":"hunter2!"}"#,
    )
    .unwrap()
}

#[test]
fn valid_register_request_passes() {
    assert!(valid_register().validate().is_ok());
}

#[test]
fn register_username_too_short_fails() {
    let req: RegisterRequest = serde_json::from_str(
        r#"{"username":"ab","email":"alice@example.com","password":"hunter2!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn register_username_too_long_fails() {
    let long_name = "a".repeat(31);
    let json = format!(r#"{{"username":"{long_name}","email":"a@b.com","password":"hunter2!"}}"#);
    let req: RegisterRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn register_invalid_email_fails() {
    let req: RegisterRequest = serde_json::from_str(
        r#"{"username":"alice","email":"not-an-email","password":"hunter2!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn register_weak_password_no_digit_fails() {
    let req: RegisterRequest = serde_json::from_str(
        r#"{"username":"alice","email":"a@b.com","password":"password!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn register_weak_password_no_special_char_fails() {
    let req: RegisterRequest = serde_json::from_str(
        r#"{"username":"alice","email":"a@b.com","password":"password1"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn register_password_too_short_fails() {
    let req: RegisterRequest = serde_json::from_str(
        r#"{"username":"alice","email":"a@b.com","password":"Ab1!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_err());
}

// ─── LoginRequest ─────────────────────────────────────────────────────────────

#[test]
fn valid_login_request_passes() {
    let req: LoginRequest = serde_json::from_str(
        r#"{"email":"alice@example.com","password":"hunter2!"}"#,
    )
    .unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn login_invalid_email_fails() {
    let req: LoginRequest =
        serde_json::from_str(r#"{"email":"notanemail","password":"anything"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn login_empty_password_fails() {
    let req: LoginRequest =
        serde_json::from_str(r#"{"email":"a@b.com","password":""}"#).unwrap();
    assert!(req.validate().is_err());
}

// ─── ForgotPasswordRequest ────────────────────────────────────────────────────

#[test]
fn valid_forgot_password_passes() {
    let req: ForgotPasswordRequest =
        serde_json::from_str(r#"{"email":"alice@example.com"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn forgot_password_invalid_email_fails() {
    let req: ForgotPasswordRequest =
        serde_json::from_str(r#"{"email":"notanemail"}"#).unwrap();
    assert!(req.validate().is_err());
}

// ─── ResetPasswordRequest ─────────────────────────────────────────────────────

#[test]
fn valid_reset_password_passes() {
    let req: ResetPasswordRequest =
        serde_json::from_str(r#"{"new_password":"hunter2!"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn reset_password_too_short_fails() {
    let req: ResetPasswordRequest =
        serde_json::from_str(r#"{"new_password":"Ab1!"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn reset_password_no_special_char_fails() {
    let req: ResetPasswordRequest =
        serde_json::from_str(r#"{"new_password":"password1"}"#).unwrap();
    assert!(req.validate().is_err());
}

// ─── JSON deserialization edge cases ─────────────────────────────────────────

#[test]
fn register_missing_username_field_fails_to_deserialize() {
    let result: Result<RegisterRequest, _> =
        serde_json::from_str(r#"{"email":"a@b.com","password":"hunter2!"}"#);
    assert!(result.is_err());
}

#[test]
fn login_missing_password_field_fails_to_deserialize() {
    let result: Result<LoginRequest, _> =
        serde_json::from_str(r#"{"email":"a@b.com"}"#);
    assert!(result.is_err());
}
