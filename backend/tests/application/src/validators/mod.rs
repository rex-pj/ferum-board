use ferum_application::validators::{
    is_reserved_slug, validate_bio, validate_display_name, validate_image_magic,
    validate_password, validate_slug_format, validate_thread_title, validate_username,
    validate_website,
};

#[cfg(test)] mod markdown;

// ─── validate_username ────────────────────────────────────────────────────────

#[test]
fn username_too_short_rejected() {
    assert!(!validate_username("ab"));
}

#[test]
fn username_minimum_length_accepted() {
    assert!(validate_username("abc"));
}

#[test]
fn username_max_30_chars_accepted() {
    assert!(validate_username(&"a".repeat(30)));
}

#[test]
fn username_over_30_chars_rejected() {
    assert!(!validate_username(&"a".repeat(31)));
}

#[test]
fn username_with_underscore_and_dash_accepted() {
    assert!(validate_username("user_name-123"));
}

#[test]
fn username_with_space_rejected() {
    assert!(!validate_username("user name"));
}

#[test]
fn username_with_at_sign_rejected() {
    assert!(!validate_username("user@name"));
}

// ─── validate_password ───────────────────────────────────────────────────────

#[test]
fn password_too_short_rejected() {
    assert!(!validate_password("Ab1!"));
}

#[test]
fn password_without_digit_rejected() {
    assert!(!validate_password("password!"));
}

#[test]
fn password_without_special_char_rejected() {
    assert!(!validate_password("password1"));
}

#[test]
fn valid_password_accepted() {
    assert!(validate_password("hunter2!"));
}

#[test]
fn password_with_only_alphanumeric_rejected() {
    assert!(!validate_password("Passw0rd"));
}

// ─── validate_slug_format ─────────────────────────────────────────────────────

#[test]
fn valid_slug_accepted() {
    assert!(validate_slug_format("my-slug-123"));
}

#[test]
fn slug_starting_with_dash_rejected() {
    assert!(!validate_slug_format("-bad-slug"));
}

#[test]
fn slug_ending_with_dash_rejected() {
    assert!(!validate_slug_format("bad-slug-"));
}

#[test]
fn slug_with_uppercase_rejected() {
    assert!(!validate_slug_format("Bad-Slug"));
}

#[test]
fn empty_slug_rejected() {
    assert!(!validate_slug_format(""));
}

// ─── is_reserved_slug ────────────────────────────────────────────────────────

#[test]
fn api_is_reserved() {
    assert!(is_reserved_slug("api"));
}

#[test]
fn admin_is_reserved() {
    assert!(is_reserved_slug("admin"));
}

#[test]
fn normal_slug_is_not_reserved() {
    assert!(!is_reserved_slug("my-category"));
}

// ─── validate_display_name ────────────────────────────────────────────────────

#[test]
fn empty_display_name_rejected() {
    assert!(!validate_display_name(""));
}

#[test]
fn display_name_within_60_chars_accepted() {
    assert!(validate_display_name(&"あ".repeat(60)));
}

#[test]
fn display_name_over_60_chars_rejected() {
    assert!(!validate_display_name(&"a".repeat(61)));
}

// ─── validate_bio ─────────────────────────────────────────────────────────────

#[test]
fn bio_within_500_chars_accepted() {
    assert!(validate_bio(&"a".repeat(500)));
}

#[test]
fn bio_over_500_chars_rejected() {
    assert!(!validate_bio(&"a".repeat(501)));
}

#[test]
fn empty_bio_accepted() {
    assert!(validate_bio(""));
}

// ─── validate_website ─────────────────────────────────────────────────────────

#[test]
fn empty_website_clears_and_is_valid() {
    assert!(validate_website(""));
}

#[test]
fn https_website_accepted() {
    assert!(validate_website("https://example.com"));
}

#[test]
fn http_website_accepted() {
    assert!(validate_website("http://example.com"));
}

#[test]
fn javascript_scheme_rejected() {
    assert!(!validate_website("javascript:evil()"));
}

#[test]
fn website_over_255_chars_rejected() {
    let url = format!("https://example.com/{}", "a".repeat(240));
    assert!(!validate_website(&url));
}

// ─── validate_image_magic ─────────────────────────────────────────────────────

#[test]
fn jpeg_magic_bytes_accepted() {
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert!(validate_image_magic(&jpeg));
}

#[test]
fn png_magic_bytes_accepted() {
    let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
    assert!(validate_image_magic(&png));
}

#[test]
fn gif89a_magic_bytes_accepted() {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[0u8; 6]);
    assert!(validate_image_magic(&gif));
}

#[test]
fn webp_magic_bytes_accepted() {
    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&[0, 0, 0, 0]);
    webp.extend_from_slice(b"WEBP");
    assert!(validate_image_magic(&webp));
}

#[test]
fn random_bytes_rejected() {
    let garbage = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B];
    assert!(!validate_image_magic(&garbage));
}

#[test]
fn data_too_short_rejected() {
    assert!(!validate_image_magic(&[0xFF, 0xD8]));
}

// ─── validate_thread_title ────────────────────────────────────────────────────
//
// Counted in characters, not bytes. The API handler used to re-check the same
// bounds against `title.len()`, which is bytes — so on a Vietnamese board, where
// a character costs 1–3 bytes, titles this validator accepts were refused before
// they ever reached it. The handler's copy is gone; these pin the semantics so a
// future one cannot quietly reintroduce the byte count.

#[test]
fn thread_title_at_the_character_limit_is_accepted() {
    assert!(validate_thread_title(&"a".repeat(255)));
}

#[test]
fn thread_title_past_the_character_limit_is_rejected() {
    assert!(!validate_thread_title(&"a".repeat(256)));
}

#[test]
fn thread_title_length_counts_characters_not_bytes() {
    // 200 Vietnamese characters — well inside the 255-character limit, and well
    // past 255 *bytes*, which is exactly the case the old handler refused.
    let vietnamese = "ế".repeat(200);
    assert!(vietnamese.len() > 255, "fixture must exceed the byte bound to be meaningful");
    assert_eq!(vietnamese.chars().count(), 200);
    assert!(validate_thread_title(&vietnamese));
}

#[test]
fn thread_title_below_the_minimum_is_rejected() {
    assert!(!validate_thread_title("abcd"));
    assert!(validate_thread_title("abcde"));
}
