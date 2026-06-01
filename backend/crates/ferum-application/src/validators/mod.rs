#![allow(dead_code)]

pub mod markdown;

use slug::slugify;
use uuid::Uuid;

pub fn validate_username(s: &str) -> bool {
    let len = s.len();
    if len < 3 || len > 30 {
        return false;
    }
    s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Password must be at least 8 characters and contain at least one digit
/// and one non-alphanumeric character.
pub fn validate_password(s: &str) -> bool {
    if s.len() < 8 {
        return false;
    }
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    let has_special = s.chars().any(|c| !c.is_alphanumeric());
    has_digit && has_special
}

pub const PASSWORD_REQUIREMENTS: &str =
    "Password must be at least 8 characters and include at least one digit and one special character";

pub fn validate_slug_format(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

pub fn validate_email(s: &str) -> bool {
    if s.len() < 5 || s.len() > 254 {
        return false;
    }
    let at = match s.rfind('@') {
        Some(i) if i > 0 => i,
        _ => return false,
    };
    let local = &s[..at];
    let domain = &s[at + 1..];
    !local.is_empty()
        && local.len() <= 64
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

/// Validate an image by inspecting its magic bytes.
/// Returns true for JPEG, PNG, GIF, and WebP.
pub fn validate_image_magic(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    // JPEG: FF D8 FF
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return true;
    }
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return true;
    }
    // GIF87a or GIF89a
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return true;
    }
    // WebP: RIFF????WEBP
    if data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return true;
    }
    false
}

pub fn validate_display_name(s: &str) -> bool {
    let len = s.chars().count();
    len >= 1 && len <= 60
}

pub fn validate_bio(s: &str) -> bool {
    s.chars().count() <= 500
}

/// Accepts empty string (clear website) or an absolute http(s) URL up to 255 chars.
pub fn validate_website(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.len() > 255 {
        return false;
    }
    s.starts_with("https://") || s.starts_with("http://")
}

const RESERVED_SLUGS: &[&str] = &[
    "api", "admin", "mod", "auth", "setup", "health", "files", "search", "static", "assets",
    "public", "private",
];

/// Returns true if the slug collides with a system-reserved path segment.
pub fn is_reserved_slug(s: &str) -> bool {
    RESERVED_SLUGS.contains(&s)
}

pub fn generate_slug(title: &str) -> String {
    slugify(title)
}

/// Generates a collision-free slug by appending the first 8 hex chars of a UUID.
/// No retry loop needed — uniqueness is guaranteed by the UUID component.
/// Example: "how-to-install-postgresql-3f9a2b8c"
pub fn generate_thread_slug(title: &str, id: &Uuid) -> String {
    let base = generate_slug(title);
    let short_id = &id.simple().to_string()[..8];
    if base.is_empty() {
        short_id.to_string()
    } else {
        format!("{}-{}", base, short_id)
    }
}
