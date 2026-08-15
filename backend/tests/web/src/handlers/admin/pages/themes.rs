use ferum_web::handlers::admin::pages::themes::{check_theme_slug, ThemeFlash};

// ─── check_theme_slug ────────────────────────────────────────────────────────
//
// The slug becomes a directory under `themes_dir` and the archive is extracted
// into it, so anything that resolves elsewhere writes into another theme.

#[test]
fn an_ordinary_slug_is_accepted() {
    for slug in ["ferum-sumi", "arcade2", "a"] {
        assert!(check_theme_slug(slug).is_ok(), "{slug:?} should be accepted");
    }
}

#[test]
fn a_slug_that_resolves_outside_its_own_directory_is_refused() {
    // `""` and `"."` both collapse `themes_dir.join(slug)` to `themes_dir` itself,
    // which the handler's `starts_with(&theme_dir)` check cannot detect.
    for slug in ["", ".", "..", "a/b", "a\\b", "/etc", "C:x"] {
        assert!(check_theme_slug(slug).is_err(), "{slug:?} must be refused");
    }
}

#[test]
fn the_built_in_theme_cannot_be_overwritten_by_an_upload() {
    // `templates/base.html` is mandatory in the archive, so this slug would
    // replace the root of every theme inheritance chain.
    let err = check_theme_slug("default").expect_err("`default` must be refused");
    assert!(err.contains("built-in"), "message should say why: {err}");
}

#[test]
fn the_refusal_message_names_the_offending_slug() {
    // The admin sees this text and has to work out what to change in theme.json.
    let err = check_theme_slug("Bad Slug").expect_err("must be refused");
    assert!(err.contains("Bad Slug"), "message should quote the slug: {err}");
}

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
