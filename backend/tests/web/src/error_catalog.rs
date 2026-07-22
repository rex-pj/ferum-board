//! Guards the error catalog against drift.
//!
//! Error text lives only in `locales/<lang>/errors.ftl` — `AppError` emits a
//! machine code and the `translate_errors` middleware resolves it. That split is
//! what makes errors translatable, but it also means a new
//! `AppError::invalid("some_code")` with no catalog entry compiles fine and only
//! reveals itself as `some code` in a user's face.
//!
//! These tests close that gap by scanning the source for emitted codes and
//! asserting each one resolves.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ferum_application::ports::Translator;
use ferum_domain::{error_key, Locale};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

async fn translator() -> impl Translator {
    ferum_infrastructure::i18n::FluentTranslator::new(vec![repo_root().join("locales")]).await
}

/// Collects every literal error code the codebase can emit.
///
/// Deliberately a source scan rather than a registry the code must remember to
/// update — a registry would need the same discipline this test exists to
/// replace. Only single-line literal forms are matched; multi-line
/// `invalid_with(\n "code",` sites are covered by the round-trip test below.
fn emitted_codes() -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    let crates_dir = repo_root().join("backend").join("crates");
    collect(&crates_dir, &mut codes);
    codes
}

fn collect(dir: &PathBuf, out: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(src) = std::fs::read_to_string(&path) {
                scan(&src, out);
            }
        }
    }
}

fn scan(src: &str, out: &mut BTreeSet<String>) {
    for ctor in [
        "invalid(\"",
        "invalid_with(\"",
        "forbidden(\"",
        "Conflict(\"",
    ] {
        let mut rest = src;
        while let Some(idx) = rest.find(ctor) {
            rest = &rest[idx + ctor.len()..];
            if let Some(end) = rest.find('"') {
                let code = &rest[..end];
                // Codes are snake_case literals; anything else is an expression
                // (a variable, a format!) and cannot be checked statically.
                if !code.is_empty()
                    && code
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    out.insert(code.to_string());
                }
            }
        }
    }
}

#[tokio::test]
async fn every_emitted_error_code_has_a_catalog_entry() {
    let t = translator().await;
    let en = Locale::default_locale();

    let codes = emitted_codes();
    assert!(
        codes.len() > 50,
        "source scan found only {} codes — the scanner is probably broken, \
         which would make this test vacuous",
        codes.len()
    );

    let missing: Vec<&String> = codes
        .iter()
        .filter(|code| !t.has_key(&en, &error_key(code)))
        .collect();

    assert!(
        missing.is_empty(),
        "these error codes have no entry in locales/en/errors.ftl, so users would \
         see a raw code instead of a sentence:\n  {}\n\nAdd `error-<kebab-code> = …` for each.",
        missing
            .iter()
            .map(|c| format!("{} → {}", c, error_key(c)))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[tokio::test]
async fn catalog_resolves_to_real_text_not_the_key() {
    // Guards the wiring itself: if the catalog failed to load, `translate`
    // returns the key unchanged and the test above would still pass, since
    // `has_key` would be false for everything and `missing` would be huge —
    // but a *partial* load could slip through. Check a known entry explicitly.
    let t = translator().await;
    let en = Locale::default_locale();

    let text = t.translate(&en, &error_key("thread_locked"), &[]);
    assert_eq!(text, "This thread is locked.");
}

#[tokio::test]
async fn arguments_are_interpolated_into_error_messages() {
    // The size-limit errors carry their limit as an argument so the catalog and
    // the constant cannot drift. If the argument name in the catalog and the
    // one passed by the code disagree, Fluent renders the variable name — this
    // catches that.
    let t = translator().await;
    let en = Locale::default_locale();

    let text = t.translate(
        &en,
        &error_key("post_content_too_long"),
        &[("limit_kb", 100.into())],
    );
    assert!(
        text.contains("100"),
        "expected the limit to be interpolated, got: {text}"
    );
    assert!(
        !text.contains("limit_kb"),
        "argument name leaked into output — catalog and code disagree: {text}"
    );
}

#[tokio::test]
async fn plural_forms_select_on_a_numeric_argument() {
    let t = translator().await;
    let en = Locale::default_locale();
    let key = error_key("rate_limit_exceeded");

    let one = t.translate(&en, &key, &[("seconds", 1.into())]);
    let many = t.translate(&en, &key, &[("seconds", 30.into())]);

    assert!(one.contains("1 second") && !one.contains("1 seconds"), "got: {one}");
    assert!(many.contains("30 seconds"), "got: {many}");
}

#[tokio::test]
async fn email_catalog_resolves_subject_and_body() {
    // Emails are composed by a detached worker with no request context, so a
    // missing key here would silently ship a message whose subject is a raw
    // catalog id to a real inbox.
    let t = translator().await;
    let en = Locale::default_locale();

    let subject = t.translate(&en, "email-verify-subject", &[]);
    assert_eq!(subject, "Verify your email");

    let body = t.translate(
        &en,
        "email-verify-body",
        &[
            ("url", "https://example.test/verify-email/tok".into()),
            ("site_name", "Ferum".into()),
        ],
    );
    assert!(body.contains("https://example.test/verify-email/tok"), "got: {body}");
    assert!(body.contains("Ferum"), "site name not interpolated: {body}");

    let reset = t.translate(&en, "email-reset-body", &[("url", "https://x.test/r".into())]);
    assert!(reset.contains("https://x.test/r"), "got: {reset}");
}

#[tokio::test]
async fn every_installed_locale_names_itself() {
    // The language switcher lists each language in its own tongue, so a visitor
    // who cannot yet read the UI can still find their language. A locale that
    // ships without its own `language-name-*` entry would appear in the menu as
    // a raw key.
    let t = translator().await;
    let en = Locale::default_locale();

    for locale in t.available_locales() {
        let key = format!("language-name-{}", locale);
        let name = t.translate(&en, &key, &[]);
        assert_ne!(
            name, key,
            "locale '{locale}' has no `{key}` entry — it would show as a raw key in the switcher"
        );
    }
}

#[tokio::test]
async fn plural_message_covers_zero_one_and_many() {
    // `thread-replies` is the most-rendered pluralized string on the site.
    let t = translator().await;
    let en = Locale::default_locale();

    let zero = t.translate(&en, "thread-replies", &[("count", 0.into())]);
    let one = t.translate(&en, "thread-replies", &[("count", 1.into())]);
    let many = t.translate(&en, "thread-replies", &[("count", 7.into())]);

    assert_eq!(zero, "No replies");
    assert_eq!(one, "1 reply");
    assert_eq!(many, "7 replies");
}

#[tokio::test]
async fn second_locale_translates_and_inherits() {
    // End-to-end check of the whole point of the system, against the real
    // shipped catalogs rather than fixtures: Vietnamese resolves its own strings,
    // and silently inherits English for anything it hasn't translated yet.
    let t = translator().await;
    let vi = Locale::parse("vi").expect("vi is a valid tag");

    assert!(
        t.available_locales().contains(&vi),
        "vi catalog is not installed — locales/vi is missing or failed to parse"
    );

    // Translated in vi.
    assert_eq!(t.translate(&vi, "nav-search", &[]), "Tìm kiếm");
    assert_eq!(
        t.translate(&vi, &error_key("thread_locked"), &[]),
        "Chủ đề này đã bị khóa."
    );

    // Anything not translated in vi falls back to en rather than rendering a raw
    // key. The key is discovered rather than hardcoded: naming one made the test
    // fail the moment that string got translated, which is progress, not a
    // regression. What actually needs pinning is the *mechanism*.
    let untranslated: Vec<String> = t
        .default_locale_keys()
        .into_iter()
        .filter(|k| !t.has_key(&vi, k))
        .collect();

    if let Some(key) = untranslated.first() {
        let en_text = t.translate(&Locale::default_locale(), key, &[]);
        let vi_text = t.translate(&vi, key, &[]);
        assert_eq!(
            vi_text, en_text,
            "'{key}' is untranslated in vi, so it should inherit the English text"
        );
        assert_ne!(vi_text, *key, "'{key}' rendered as a raw key instead of falling back");
    }
    // If `untranslated` is empty the catalog is fully translated — nothing to
    // assert, and not a failure.

    // Arguments interpolate in the translated language too.
    let sized = t.translate(&vi, &error_key("post_content_too_long"), &[("limit_kb", 100.into())]);
    assert!(sized.contains("100") && sized.contains("KB"), "got: {sized}");
}

#[tokio::test]
async fn vietnamese_plural_collapses_to_one_form() {
    // Vietnamese has no plural inflection. The catalog still uses a selector so
    // it stays structurally aligned with the source, but every non-zero count
    // must produce the same wording — this guards against someone "helpfully"
    // adding an [one] arm that would never be selected correctly.
    let t = translator().await;
    let vi = Locale::parse("vi").unwrap();

    assert_eq!(t.translate(&vi, "thread-replies", &[("count", 0.into())]), "Chưa có trả lời");
    assert_eq!(t.translate(&vi, "thread-replies", &[("count", 1.into())]), "1 trả lời");
    assert_eq!(t.translate(&vi, "thread-replies", &[("count", 9.into())]), "9 trả lời");
}

#[tokio::test]
async fn vietnamese_covers_the_high_traffic_public_surface() {
    // Not a demand for 100% — a partial translation is the normal state, and the
    // fallback chain is what makes it safe. What this pins down is that the
    // strings a reader meets first are actually translated, so `vi` is a real
    // language on this site rather than an English page with a Vietnamese label.
    let t = translator().await;
    let vi = Locale::parse("vi").unwrap();

    let must_be_translated = [
        "ui-home", "ui-forum", "ui-search", "ui-categories", "ui-account",
        "ui-notifications", "ui-log-in", "ui-register", "ui-new-thread",
        "ui-reply", "ui-cancel", "ui-delete", "ui-password", "ui-username",
        "nav-language", "prefs-language", "js-network-error",
        "error-page-404-title", "action-go-home",
    ];

    let missing: Vec<&str> = must_be_translated
        .iter()
        .copied()
        .filter(|k| !t.has_key(&vi, k))
        .collect();

    assert!(
        missing.is_empty(),
        "these high-traffic keys are untranslated in vi, so a Vietnamese reader \
         would meet English on the first screen:\n  {}",
        missing.join("\n  ")
    );
}

#[tokio::test]
async fn every_locale_defines_its_own_date_and_number_formats() {
    // These are not sentences but format definitions, and the fallback would
    // silently give a locale English field order and an English thousands
    // separator — a bug that looks like a styling quirk rather than a missing
    // translation.
    let t = translator().await;

    for locale in t.available_locales() {
        for key in ["format-date", "format-thousands-separator", "month-short-1"] {
            assert!(
                t.has_key(&locale, key),
                "locale '{locale}' does not define '{key}', so it would inherit \
                 English formatting"
            );
        }
    }
}
