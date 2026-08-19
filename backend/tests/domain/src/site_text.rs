//! `site_text` — per-locale copy over the flat `site_config` key space.
//!
//! Two properties are worth pinning, both because their failure is silent. A key the
//! settings page writes must be one `update_config` accepts — otherwise the save
//! returns 200 and changes nothing. And resolution must end at the bare key —
//! otherwise a language with no translation gets a blank `<meta name="description">`
//! on every page.

use std::collections::HashMap;

use ferum_domain::site_text::{
    is_writable_localized_key, localized_key, resolve, LOCALIZED_CONFIG_KEYS,
};
use ferum_domain::Locale;

fn locale(tag: &str) -> Locale {
    Locale::parse(tag).unwrap_or_else(|| panic!("{tag} must be a valid tag"))
}

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn the_default_locale_keeps_the_bare_key() {
    // The bare key is what the setup wizard writes and what every install already
    // has. Suffixing it would orphan that value behind a key nothing reads, and the
    // symptom is a site whose tagline silently empties on upgrade.
    assert_eq!(
        localized_key("site_tagline", &Locale::default_locale()),
        "site_tagline"
    );
    assert_eq!(
        localized_key("site_tagline", &locale("vi")),
        "site_tagline:vi"
    );
}

#[test]
fn every_key_the_settings_page_writes_is_one_update_config_accepts() {
    // The settings page composes its field ids with `localized_key` and
    // `update_config` filters with `is_writable_localized_key`. These are the two
    // halves of one contract, and a mismatch is a save that reports success and
    // drops the value.
    for base in LOCALIZED_CONFIG_KEYS {
        for tag in ["vi", "ja", "en-US", "zh-Hant-TW"] {
            let key = localized_key(base, &locale(tag));
            assert!(
                is_writable_localized_key(&key),
                "the settings page would render cfg-{key}, which update_config drops"
            );
        }
    }
}

#[test]
fn only_a_canonical_localized_key_is_accepted() {
    // Two spellings of one language would be two rows, and only one of them would
    // ever be read — the other is an edit that appears to save and does nothing.
    // A bare key is the static allowlist's business, so it must not match here.
    for rejected in [
        "site_tagline",
        "site_tagline:en",
        "site_tagline:EN-us",
        "site_tagline:VI",
        "site_tagline:en_US",
        "site_tagline:",
        "site_tagline:not-a-locale",
        "site_tagline:../etc/passwd",
        ":vi",
    ] {
        assert!(
            !is_writable_localized_key(rejected),
            "{rejected} must not be accepted as a localized key"
        );
    }
}

#[test]
fn a_locale_suffix_does_not_make_an_arbitrary_key_writable() {
    // Without the base-key check the suffix would be a bypass for the writable-key
    // allowlist — including for the one key that must never be echoed back.
    for rejected in ["smtp_pass:vi", "registration_open:vi", "enabled_locales:vi"] {
        assert!(
            !is_writable_localized_key(rejected),
            "{rejected} is not localizable copy and must not become writable"
        );
    }
}

#[test]
fn resolution_prefers_the_locales_own_copy() {
    let cfg = config(&[
        ("site_tagline", "A modern self-hosted forum"),
        ("site_tagline:vi", "Diễn đàn tự vận hành"),
    ]);
    assert_eq!(
        resolve(&cfg, "site_tagline", &locale("vi")),
        "Diễn đàn tự vận hành"
    );
    assert_eq!(
        resolve(&cfg, "site_tagline", &Locale::default_locale()),
        "A modern self-hosted forum"
    );
}

#[test]
fn a_missing_translation_falls_back_to_the_bare_key() {
    // The fallback is what keeps the tagline — every page's meta description —
    // non-empty for a language nobody has translated yet.
    let cfg = config(&[("site_tagline", "A modern self-hosted forum")]);
    assert_eq!(
        resolve(&cfg, "site_tagline", &locale("vi")),
        "A modern self-hosted forum"
    );
}

#[test]
fn a_regional_locale_inherits_its_base_language() {
    // `vi-VN` → `vi` → default. A regional variant only needs its own copy where it
    // actually differs, matching how every other catalog in the system resolves.
    let cfg = config(&[("site_tagline", "base"), ("site_tagline:vi", "tiếng Việt")]);
    assert_eq!(resolve(&cfg, "site_tagline", &locale("vi-VN")), "tiếng Việt");
}

#[test]
fn a_blank_translation_falls_through_rather_than_blanking_the_page() {
    // An admin who clears the field posts `""`, and `update_config` stores it. Left
    // as a hit, that would blank the meta description of every page in that language
    // while the default-locale copy sat right there unused.
    let cfg = config(&[("site_tagline", "base"), ("site_tagline:vi", "")]);
    assert_eq!(resolve(&cfg, "site_tagline", &locale("vi")), "base");
}

#[test]
fn nothing_stored_resolves_to_nothing() {
    // The template contract is a plain string that renders to nothing when unset.
    assert_eq!(resolve(&config(&[("site_tagline", "")]), "site_tagline", &locale("vi")), "");
    assert_eq!(resolve(&HashMap::new(), "site_tagline", &locale("vi")), "");
}
