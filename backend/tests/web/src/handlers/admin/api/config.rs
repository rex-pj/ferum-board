//! The site-config key allowlists, and the SMTP block parsed out of them.
//!
//! No request structs are defined in this handler — it takes a free-form
//! `HashMap<String, String>` — so what needs guarding is the three key lists and
//! `smtp_settings_from_config`, none of which had any coverage. Between them they
//! decide which settings can be written, which are echoed back, and which are
//! secret; getting any of the three wrong is silent in a different way.

use std::collections::HashMap;

use ferum_web::handlers::admin::api::config::{
    smtp_settings_from_config, split_secrets, CONFIG_SECRET_KEYS,
};

fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ─── smtp_settings_from_config ───────────────────────────────────────────────

#[test]
fn no_host_means_not_configured() {
    assert!(smtp_settings_from_config(&cfg(&[])).unwrap().is_none());
}

#[test]
fn a_blank_or_whitespace_host_means_not_configured() {
    // `PgSystemSeedService` seeds `smtp_host = ""`, and an admin clearing the field
    // stores the same. Both have to read as "off" rather than as a host named "".
    for host in ["", "   ", "\t\n"] {
        assert!(
            smtp_settings_from_config(&cfg(&[("smtp_host", host)]))
                .unwrap()
                .is_none(),
            "host {host:?} should count as unconfigured"
        );
    }
}

#[test]
fn an_absent_port_defaults_to_the_submission_port() {
    let settings = smtp_settings_from_config(&cfg(&[("smtp_host", "smtp.example.com")]))
        .unwrap()
        .expect("configured");
    assert_eq!(settings.port, 587);
}

#[test]
fn a_bad_port_is_an_error_rather_than_a_silent_default() {
    // Must fail the request. `update_config` validates before writing precisely so
    // an unusable value cannot persist and leave mail broken while the settings
    // page reported a successful save.
    for port in ["not-a-number", "70000", "-1", "587.5"] {
        assert!(
            smtp_settings_from_config(&cfg(&[
                ("smtp_host", "smtp.example.com"),
                ("smtp_port", port),
            ]))
            .is_err(),
            "port {port:?} should be rejected"
        );
    }
}

#[test]
fn a_blank_port_falls_back_instead_of_erroring() {
    // Distinct from the case above: blank is "not set", not "set to nonsense".
    let settings = smtp_settings_from_config(&cfg(&[
        ("smtp_host", "smtp.example.com"),
        ("smtp_port", "   "),
    ]))
    .unwrap()
    .expect("configured");
    assert_eq!(settings.port, 587);
}

#[test]
fn blank_credentials_become_none_rather_than_empty_strings() {
    // `LettreEmailService` attaches credentials only when both are `Some`, so an
    // empty-string username would make it authenticate as "" instead of
    // anonymously.
    let settings = smtp_settings_from_config(&cfg(&[
        ("smtp_host", "smtp.example.com"),
        ("smtp_user", ""),
        ("smtp_pass", "   "),
    ]))
    .unwrap()
    .expect("configured");
    assert!(settings.username.is_none());
    assert!(settings.password.is_none());
}

#[test]
fn host_and_credentials_are_trimmed() {
    let settings = smtp_settings_from_config(&cfg(&[
        ("smtp_host", "  smtp.example.com  "),
        ("smtp_user", " apikey "),
        ("smtp_pass", " s3cret "),
    ]))
    .unwrap()
    .expect("configured");
    // A trailing space in a hostname is a DNS failure, and in a password an auth
    // failure — both from a value that looks right in the form.
    assert_eq!(settings.host, "smtp.example.com");
    assert_eq!(settings.username.as_deref(), Some("apikey"));
    assert_eq!(settings.password.as_deref(), Some("s3cret"));
}

// ─── split_secrets ───────────────────────────────────────────────────────────

#[test]
fn split_secrets_removes_the_value_and_reports_presence() {
    let (safe, flags) = split_secrets(cfg(&[
        ("site_name", "Ferum"),
        ("smtp_host", "smtp.example.com"),
        ("smtp_pass", "s3cret"),
    ]));

    // The secret must not survive into anything a template can reach.
    assert!(!safe.contains_key("smtp_pass"));
    assert!(!safe.values().any(|v| v == "s3cret"));
    // Everything else is untouched.
    assert_eq!(safe.get("site_name").map(String::as_str), Some("Ferum"));
    assert_eq!(
        safe.get("smtp_host").map(String::as_str),
        Some("smtp.example.com")
    );
    assert_eq!(flags.get("smtp_pass"), Some(&true));
}

#[test]
fn a_blank_secret_reports_as_not_set() {
    // The seeded state, and the state after an admin clears the field. Reporting
    // "(set)" here would tell an operator a password exists when none does.
    for value in ["", "   "] {
        let (safe, flags) = split_secrets(cfg(&[("smtp_pass", value)]));
        assert!(!safe.contains_key("smtp_pass"));
        assert_eq!(flags.get("smtp_pass"), Some(&false), "value {value:?}");
    }
}

#[test]
fn an_absent_secret_reports_as_not_set() {
    let (_safe, flags) = split_secrets(cfg(&[("site_name", "Ferum")]));
    assert_eq!(flags.get("smtp_pass"), Some(&false));
}

#[test]
fn every_secret_key_is_reported() {
    // Adding a key to CONFIG_SECRET_KEYS without a flag for it would strip the
    // value and leave the template with no way to say whether it is set.
    let all = cfg(&CONFIG_SECRET_KEYS
        .iter()
        .map(|k| (*k, "value"))
        .collect::<Vec<_>>());
    let (safe, flags) = split_secrets(all);
    for key in CONFIG_SECRET_KEYS {
        assert!(!safe.contains_key(*key), "{key} was not stripped");
        assert_eq!(flags.get(*key), Some(&true), "{key} has no presence flag");
    }
}
