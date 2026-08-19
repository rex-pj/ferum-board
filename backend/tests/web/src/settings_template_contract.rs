//! Source-scanning guards over `admin/settings.html` and the macros it
//! imports.
//!
//! Each encodes a rule the codebase already states in prose but nothing enforced.
//! Scanning the templates rather than maintaining a list is the same choice
//! `template_keys.rs` makes: a list would need the discipline the test exists to
//! replace.

use std::path::PathBuf;

use ferum_domain::site_text::LOCALIZED_CONFIG_KEYS;
use ferum_web::handlers::admin::api::config::{CONFIG_SECRET_KEYS, CONFIG_WRITABLE_KEYS};

fn admin_template(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("frontend")
        .join("templates")
        .join("admin")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn settings_html() -> String {
    admin_template("settings.html")
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
fn every_static_cfg_field_on_the_settings_page_is_writable() {
    // `update_config` silently drops any key absent from `CONFIG_WRITABLE_KEYS`, so
    // a field rendered here and missing from that list is a save that reports
    // success and does nothing. The handler's own doc comment warns about it; this
    // makes the warning enforceable.
    //
    // `macros.html` is scanned too — it holds the translations disclosure, and a
    // static field added there would otherwise be covered by nothing. Its per-locale
    // ids are `cfg-{{ f.key }}`, which no source scan can resolve; those are checked
    // against the rendered page by `admin_settings_render.rs`
    // (`every_rendered_cfg_field_is_writable`).
    let sources = [
        ("settings.html", settings_html()),
        ("macros.html", admin_template("macros.html")),
    ];

    let mut total_found = 0usize;
    for (name, html) in &sources {
        for (index, _) in html.match_indices("id=\"cfg-") {
            let rest = &html[index + "id=\"cfg-".len()..];
            let end = rest
                .find('"')
                .unwrap_or_else(|| panic!("{name}: unterminated id attribute at byte {index}"));
            let key = &rest[..end];
            total_found += 1;
            if key.contains("{{") {
                continue; // interpolated — the render test owns it
            }
            assert!(
                CONFIG_WRITABLE_KEYS.contains(&key),
                "admin/{name} renders `cfg-{key}` but `{key}` is not in \
                 CONFIG_WRITABLE_KEYS, so saving it would appear to succeed and change nothing."
            );
        }
    }

    assert!(
        total_found > 15,
        "expected to find the settings fields; found {total_found} — the scan is probably \
         broken rather than the templates being empty"
    );
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

#[test]
fn every_localizable_key_is_writable_in_its_bare_form_too() {
    // The bare key is the default-locale copy AND the fallback for every language, so
    // a key on the localized list but off `CONFIG_WRITABLE_KEYS` would save its
    // translations while silently dropping the original — a field where typing
    // English does nothing and typing Vietnamese works.
    for base in LOCALIZED_CONFIG_KEYS {
        assert!(
            CONFIG_WRITABLE_KEYS.contains(base),
            "`{base}` is in site_text::LOCALIZED_CONFIG_KEYS but not in \
             CONFIG_WRITABLE_KEYS, so its default-locale copy cannot be saved."
        );
    }
}
