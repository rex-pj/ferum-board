//! Two source-scanning guards over `admin/settings.html`.
//!
//! Both encode a rule the codebase already states in prose but nothing enforced.
//! Scanning the template rather than maintaining a list is the same choice
//! `template_keys.rs` makes: a list would need the discipline the test exists to
//! replace.

use std::path::PathBuf;

use ferum_web::handlers::admin::api::config::{CONFIG_SECRET_KEYS, CONFIG_WRITABLE_KEYS};

fn settings_html() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("frontend")
        .join("templates")
        .join("admin")
        .join("settings.html");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

#[test]
fn the_settings_page_never_references_a_secret_config_value() {
    // The page renders `site_config` wholesale rather than through
    // `CONFIG_READABLE_KEYS`, so the allowlist that guards `GET /api/admin/config`
    // does not apply to it at all. The handler strips secrets and passes presence
    // flags instead; this is what stops a later edit from reaching for the value
    // again — which is exactly how the leak arose the first time, with the template
    // testing `{% if config.smtp_pass %}` for a boolean it did not need the secret
    // to compute.
    let html = settings_html();
    for key in CONFIG_SECRET_KEYS {
        let forbidden = format!("config.{key}");
        assert!(
            !html.contains(&forbidden),
            "admin/settings.html references `{forbidden}`. Secret values are removed from \
             the template context by `split_secrets`; use the presence flag \
             (`{key}_set`) instead."
        );
    }
}

#[test]
fn every_cfg_field_on_the_settings_page_is_writable() {
    // `update_config` silently drops any key absent from `CONFIG_WRITABLE_KEYS`, so
    // a field rendered here and missing from that list is a save that reports
    // success and does nothing. The handler's own doc comment warns about it; this
    // makes the warning enforceable.
    let html = settings_html();

    let mut found = Vec::new();
    for (index, _) in html.match_indices("id=\"cfg-") {
        let rest = &html[index + "id=\"cfg-".len()..];
        let end = rest
            .find('"')
            .unwrap_or_else(|| panic!("unterminated id attribute at byte {index}"));
        found.push(&rest[..end]);
    }

    assert!(
        found.len() > 15,
        "expected to find the settings fields; found {} — the scan is probably broken \
         rather than the template being empty",
        found.len()
    );

    for key in &found {
        assert!(
            CONFIG_WRITABLE_KEYS.contains(key),
            "admin/settings.html renders `cfg-{key}` but `{key}` is not in \
             CONFIG_WRITABLE_KEYS, so saving it would appear to succeed and change nothing."
        );
    }
}

#[test]
fn the_email_tab_branches_on_the_provider_labels_the_service_emits() {
    // `mail_provider` is inserted as `MailProvider::label()`, and the template
    // compares it against string literals. A renamed label would silently take
    // every branch to the `else`, which reports "not configured" on a forum that
    // sends mail perfectly well.
    let html = settings_html();
    for label in ["resend", "smtp", "disabled"] {
        assert!(
            html.contains(&format!("mail_provider == \"{label}\"")),
            "admin/settings.html has no branch for mail_provider == {label:?}"
        );
    }
}
