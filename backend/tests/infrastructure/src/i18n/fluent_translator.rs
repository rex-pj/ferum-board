//! `FluentTranslator` behaviour, exercised through the `Translator` port.
//!
//! Everything here goes through the public surface — `FluentTranslator::new` and
//! the trait methods — rather than reaching into `load_catalogs` or the private
//! `Catalogs` struct. That is deliberate: the catalog roster is already
//! observable as `available_locales()`, so testing the internal type would only
//! pin an implementation detail while asserting the same fact.

use std::path::PathBuf;

use ferum_application::ports::{TransArg, Translator};
use ferum_domain::Locale;
use ferum_infrastructure::i18n::FluentTranslator;

/// Writes a throwaway locale tree and cleans it up on drop.
struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ferum-i18n-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Fixture(dir)
    }

    fn write(&self, locale: &str, file: &str, body: &str) {
        let dir = self.0.join(locale);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), body).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn translator_over(roots: Vec<PathBuf>) -> FluentTranslator {
    FluentTranslator::new(roots).await
}

#[tokio::test]
async fn loads_and_formats_a_simple_message() {
    let f = Fixture::new("simple");
    f.write("en", "errors.ftl", "error-thread-locked = This thread is locked.\n");

    let t = translator_over(vec![f.0.clone()]).await;
    assert_eq!(
        t.translate(&Locale::default_locale(), "error-thread-locked", &[]),
        "This thread is locked."
    );
}

#[tokio::test]
async fn interpolates_arguments() {
    let f = Fixture::new("args");
    f.write("en", "c.ftl", "greet = Hello, { $name }!\n");

    let t = translator_over(vec![f.0.clone()]).await;
    assert_eq!(
        t.translate(
            &Locale::default_locale(),
            "greet",
            &[("name", TransArg::Str("Trung".into()))]
        ),
        "Hello, Trung!"
    );
}

#[tokio::test]
async fn selects_plural_form_from_a_numeric_argument() {
    // The reason TransArg distinguishes Int from Str: a string "2" would
    // fall through to the catch-all arm instead of matching `[one]`.
    let f = Fixture::new("plural");
    f.write(
        "en",
        "c.ftl",
        "replies = { $count ->\n    [one] 1 reply\n   *[other] { $count } replies\n  }\n",
    );

    let t = translator_over(vec![f.0.clone()]).await;
    let en = Locale::default_locale();
    assert_eq!(t.translate(&en, "replies", &[("count", TransArg::Int(1))]), "1 reply");
    assert_eq!(t.translate(&en, "replies", &[("count", TransArg::Int(5))]), "5 replies");
}

#[tokio::test]
async fn falls_back_to_default_locale_for_untranslated_keys() {
    let f = Fixture::new("fallback");
    f.write("en", "c.ftl", "only-in-en = English text\nshared = English shared\n");
    f.write("vi", "c.ftl", "shared = Tiếng Việt\n");

    let t = translator_over(vec![f.0.clone()]).await;
    let vi = Locale::parse("vi").unwrap();

    assert_eq!(t.translate(&vi, "shared", &[]), "Tiếng Việt");
    // Untranslated in vi → resolves through the chain to en.
    assert_eq!(t.translate(&vi, "only-in-en", &[]), "English text");
}

#[tokio::test]
async fn regional_locale_narrows_to_its_base_language() {
    let f = Fixture::new("regional");
    f.write("en", "c.ftl", "colour = color\nshared = base\n");
    f.write("en-GB", "c.ftl", "colour = colour\n");

    let t = translator_over(vec![f.0.clone()]).await;
    let gb = Locale::parse("en-GB").unwrap();

    assert_eq!(t.translate(&gb, "colour", &[]), "colour");
    // Not overridden regionally → falls back to `en`.
    assert_eq!(t.translate(&gb, "shared", &[]), "base");
}

#[tokio::test]
async fn missing_key_renders_the_key_rather_than_empty_string() {
    let f = Fixture::new("missing");
    f.write("en", "c.ftl", "present = yes\n");

    let t = translator_over(vec![f.0.clone()]).await;
    assert_eq!(
        t.translate(&Locale::default_locale(), "totally-absent", &[]),
        "totally-absent"
    );
}

#[tokio::test]
async fn later_root_overrides_earlier_one() {
    // Core ships a string; a theme shadows it. Same precedence direction as
    // the theme template chain.
    let core = Fixture::new("core");
    let theme = Fixture::new("theme");
    core.write("en", "c.ftl", "site-title = Ferum Board\ncore-only = kept\n");
    theme.write("en", "c.ftl", "site-title = My Community\n");

    let t = translator_over(vec![core.0.clone(), theme.0.clone()]).await;
    let en = Locale::default_locale();

    assert_eq!(t.translate(&en, "site-title", &[]), "My Community");
    assert_eq!(t.translate(&en, "core-only", &[]), "kept");
}

#[tokio::test]
async fn has_key_distinguishes_own_translation_from_inherited() {
    let f = Fixture::new("haskey");
    f.write("en", "c.ftl", "a = A\nb = B\n");
    f.write("vi", "c.ftl", "a = A-vi\n");

    let t = translator_over(vec![f.0.clone()]).await;
    let vi = Locale::parse("vi").unwrap();

    assert!(t.has_key(&vi, "a"));
    // Resolvable via fallback, but NOT translated in vi — coverage must
    // count this as missing or the admin percentage is meaningless.
    assert!(!t.has_key(&vi, "b"));
    assert_eq!(t.translate(&vi, "b", &[]), "B");
}

#[tokio::test]
async fn invalid_locale_directory_is_skipped() {
    let f = Fixture::new("badlocale");
    f.write("en", "c.ftl", "a = A\n");
    f.write("not-a-locale-at-all", "c.ftl", "a = bad\n");

    let t = translator_over(vec![f.0.clone()]).await;
    assert_eq!(t.available_locales(), vec![Locale::default_locale()]);
}

#[tokio::test]
async fn default_locale_sorts_first_in_the_roster() {
    let f = Fixture::new("order");
    for l in ["ar", "en", "vi", "de"] {
        f.write(l, "c.ftl", "a = A\n");
    }

    let t = translator_over(vec![f.0.clone()]).await;
    let order = t.available_locales();
    assert_eq!(order[0], Locale::default_locale());
    assert_eq!(order.len(), 4);
}

#[tokio::test]
async fn malformed_message_does_not_discard_the_rest_of_the_file() {
    let f = Fixture::new("partial");
    f.write("en", "c.ftl", "good-one = fine\n= broken\ngood-two = also fine\n");

    let t = translator_over(vec![f.0.clone()]).await;
    let en = Locale::default_locale();
    assert_eq!(t.translate(&en, "good-one", &[]), "fine");
    assert_eq!(t.translate(&en, "good-two", &[]), "also fine");
}

#[tokio::test]
async fn no_bidi_isolation_marks_in_output() {
    // set_use_isolating(false) — otherwise every interpolated value is
    // wrapped in U+2068/U+2069 and string comparisons mysteriously fail.
    let f = Fixture::new("isolating");
    f.write("en", "c.ftl", "hi = Hi { $name }\n");

    let t = translator_over(vec![f.0.clone()]).await;
    let out = t.translate(&Locale::default_locale(), "hi", &[("name", "Bo".into())]);

    assert_eq!(out, "Hi Bo");
    assert!(!out.contains('\u{2068}'), "unexpected bidi isolate in {out:?}");
}
