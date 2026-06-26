use validator::ValidationError;

use ferum_application::validators as v;

/// Enforces digit + special char + min-8 length (wraps validate_password).
pub fn password_complexity(val: &str) -> Result<(), ValidationError> {
    if !v::validate_password(val) {
        let mut err = ValidationError::new("password_complexity");
        err.message = Some(std::borrow::Cow::Borrowed(v::PASSWORD_REQUIREMENTS));
        return Err(err);
    }
    Ok(())
}

/// Enforces lowercase-alphanumeric-hyphen slug format (wraps validate_slug_format).
pub fn slug_format(val: &str) -> Result<(), ValidationError> {
    if !v::validate_slug_format(val) {
        let mut err = ValidationError::new("invalid_slug");
        err.message = Some(std::borrow::Cow::Borrowed(
            "Slug must be lowercase letters, digits, and hyphens only, with no leading or trailing hyphens",
        ));
        return Err(err);
    }
    Ok(())
}

/// Validates that a color string is a CSS hex color (#RGB or #RRGGBB).
pub fn hex_color(val: &str) -> Result<(), ValidationError> {
    let stripped = val.strip_prefix('#').unwrap_or("");
    if matches!(stripped.len(), 3 | 6) && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }
    let mut err = ValidationError::new("invalid_hex_color");
    err.message = Some(std::borrow::Cow::Borrowed(
        "Color must be a CSS hex value like #RGB or #RRGGBB",
    ));
    Err(err)
}

/// Validates a report resolution status value ("resolved" or "dismissed").
pub fn valid_report_status(val: &str) -> Result<(), ValidationError> {
    if !matches!(val, "resolved" | "dismissed") {
        let mut err = ValidationError::new("invalid_status");
        err.message = Some(std::borrow::Cow::Borrowed(
            "status must be 'resolved' or 'dismissed'",
        ));
        return Err(err);
    }
    Ok(())
}

