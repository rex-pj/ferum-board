use ferum_web::view_models::validators::{hex_color, password_complexity, slug_format, valid_report_status};

// ─── password_complexity ──────────────────────────────────────────────────────

#[test]
fn valid_password_returns_ok() {
    assert!(password_complexity("hunter2!").is_ok());
}

#[test]
fn password_without_digit_returns_err() {
    assert!(password_complexity("password!").is_err());
}

#[test]
fn password_without_special_char_returns_err() {
    assert!(password_complexity("password1").is_err());
}

#[test]
fn password_below_min_length_returns_err() {
    assert!(password_complexity("Ab1!").is_err());
}

#[test]
fn password_error_uses_expected_code() {
    let err = password_complexity("bad").unwrap_err();
    assert_eq!(err.code.as_ref(), "password_complexity");
}

// ─── slug_format ─────────────────────────────────────────────────────────────

#[test]
fn valid_slug_returns_ok() {
    assert!(slug_format("my-slug-123").is_ok());
}

#[test]
fn uppercase_slug_returns_err() {
    assert!(slug_format("My-Slug").is_err());
}

#[test]
fn slug_with_leading_dash_returns_err() {
    assert!(slug_format("-bad").is_err());
}

#[test]
fn slug_with_trailing_dash_returns_err() {
    assert!(slug_format("bad-").is_err());
}

#[test]
fn slug_with_space_returns_err() {
    assert!(slug_format("with space").is_err());
}

#[test]
fn empty_slug_returns_err() {
    assert!(slug_format("").is_err());
}

#[test]
fn slug_with_numbers_only_returns_ok() {
    assert!(slug_format("123").is_ok());
}

// ─── hex_color ───────────────────────────────────────────────────────────────

#[test]
fn three_char_hex_returns_ok() {
    assert!(hex_color("#abc").is_ok());
}

#[test]
fn six_char_hex_returns_ok() {
    assert!(hex_color("#1a2b3c").is_ok());
}

#[test]
fn uppercase_hex_returns_ok() {
    assert!(hex_color("#AABBCC").is_ok());
}

#[test]
fn hex_without_hash_prefix_returns_err() {
    assert!(hex_color("aabbcc").is_err());
}

#[test]
fn four_char_hex_returns_err() {
    assert!(hex_color("#abcd").is_err());
}

#[test]
fn seven_char_hex_returns_err() {
    assert!(hex_color("#aabbccd").is_err());
}

#[test]
fn non_hex_chars_return_err() {
    assert!(hex_color("#zzzzzz").is_err());
}

#[test]
fn empty_string_returns_err() {
    assert!(hex_color("").is_err());
}

// ─── valid_report_status ─────────────────────────────────────────────────────

#[test]
fn resolved_is_valid() {
    assert!(valid_report_status("resolved").is_ok());
}

#[test]
fn dismissed_is_valid() {
    assert!(valid_report_status("dismissed").is_ok());
}

#[test]
fn pending_is_not_valid() {
    assert!(valid_report_status("pending").is_err());
}

#[test]
fn arbitrary_string_is_not_valid() {
    assert!(valid_report_status("approved").is_err());
}

#[test]
fn empty_string_is_not_valid() {
    assert!(valid_report_status("").is_err());
}
