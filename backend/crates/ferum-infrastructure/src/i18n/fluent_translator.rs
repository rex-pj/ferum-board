//! Fluent-backed implementation of the `Translator` port.
//!
//! Catalogs are plain `.ftl` files on disk, grouped by locale:
//!
//! ```text
//! locales/
//!   en/  errors.ftl  common.ftl  forum.ftl
//!   vi/  errors.ftl  common.ftl
//! ```
//!
//! Every `.ftl` in a locale directory is merged into one bundle, so splitting by
//! domain is purely an authoring convenience — key names, not filenames, are the
//! namespace.
//!
//! ## Key naming
//!
//! Fluent message identifiers are `[a-zA-Z][a-zA-Z0-9_-]*` — **dots are not
//! legal**. Keys are therefore kebab-case with the namespace as a prefix
//! (`error-thread-locked`, `forum-reply-button`), not dotted paths.
//!
//! ## Concurrency
//!
//! The bundle is built with `new_concurrent`, which swaps Fluent's default
//! `RefCell`-based memoizer for a thread-safe one. The default memoizer is not
//! `Sync`, so a plain `FluentBundle` cannot live in `AppState` at all.
//!
//! Catalogs sit behind a `std::sync::RwLock<Arc<..>>` rather than a
//! `tokio::sync::RwLock` on purpose: `translate` is called from inside Tera's
//! synchronous `register_function` closure and cannot await. Readers clone the
//! `Arc` and release the lock immediately, so a reload never blocks rendering —
//! in-flight renders finish against the old catalog, exactly like the theme
//! hot-reload's `Arc<Tera>` swap.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use fluent_syntax::ast;

use ferum_application::ports::{TransArg, Translator};
use ferum_application::shared::AppError;
use ferum_domain::Locale;

/// Thread-safe bundle. `FluentBundle`'s default memoizer is `RefCell`-backed and
/// therefore not `Sync`; the concurrent one is what makes this shareable.
type Bundle = fluent_bundle::bundle::FluentBundle<
    FluentResource,
    intl_memoizer::concurrent::IntlLangMemoizer,
>;

struct LocaleCatalog {
    bundle: Bundle,
    /// Message ids defined *in this locale's own files*. Needed because
    /// `has_key` must distinguish "translated here" from "inherited via
    /// fallback", and the bundle itself cannot answer that.
    keys: HashSet<String>,
}

impl LocaleCatalog {
    /// Formats `key`, or `None` if this catalog does not define it.
    fn format(&self, key: &str, args: &[(&str, TransArg)]) -> Option<String> {
        let message = self.bundle.get_message(key)?;
        let pattern = message.value()?;

        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(
                name.to_string(),
                match value {
                    TransArg::Str(s) => FluentValue::from(s.as_str()),
                    TransArg::Int(i) => FluentValue::from(*i),
                    TransArg::Float(f) => FluentValue::from(*f),
                },
            );
        }

        let mut errors = Vec::new();
        let out = self
            .bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors);

        // Formatting errors mean a placeable referenced an argument the caller
        // did not supply. Fluent still returns usable text (it renders the
        // variable name in place), so this is a loud warning, not a failure.
        if !errors.is_empty() {
            tracing::warn!(
                i18n.key = key,
                i18n.errors = ?errors,
                "translation formatting produced errors"
            );
        }

        Some(out.into_owned())
    }
}

struct Catalogs {
    by_locale: HashMap<Locale, LocaleCatalog>,
    /// Installed locales, default first — the order `available_locales` reports.
    order: Vec<Locale>,
    /// Canonical key set, from the default locale. Coverage percentages in the
    /// admin UI are computed against this.
    default_keys: Vec<String>,
}

impl Catalogs {
    fn empty() -> Self {
        Catalogs {
            by_locale: HashMap::new(),
            order: Vec::new(),
            default_keys: Vec::new(),
        }
    }
}

pub struct FluentTranslator {
    catalogs: RwLock<Arc<Catalogs>>,
    /// Catalog roots in ascending precedence: core first, then theme, then
    /// plugin. A later root overrides a key defined by an earlier one, which is
    /// the same precedence chain the theme template resolver uses.
    roots: Vec<PathBuf>,
}

impl FluentTranslator {
    /// Builds a translator and loads catalogs immediately.
    ///
    /// A missing or unreadable root is logged and skipped rather than fatal: an
    /// untranslated site is a degraded site, but a site that refuses to boot
    /// because a language pack directory is absent is an outage.
    pub async fn new(roots: Vec<PathBuf>) -> Self {
        let translator = FluentTranslator {
            catalogs: RwLock::new(Arc::new(Catalogs::empty())),
            roots,
        };
        if let Err(e) = translator.reload().await {
            tracing::error!(error = %e, "initial translation catalog load failed");
        }
        translator
    }

    fn snapshot(&self) -> Arc<Catalogs> {
        // Clone the Arc and drop the guard immediately — never hold the lock
        // across formatting.
        match self.catalogs.read() {
            Ok(guard) => Arc::clone(&guard),
            // A poisoned lock means a writer panicked mid-swap. Recover the
            // value rather than propagating a panic into every page render.
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }
}

#[async_trait]
impl Translator for FluentTranslator {
    fn translate(&self, locale: &Locale, key: &str, args: &[(&str, TransArg)]) -> String {
        let catalogs = self.snapshot();

        // `fallback_chain` already terminates at the default locale, so this
        // covers "regional narrows to base" and "anything narrows to default"
        // in one pass.
        for candidate in locale.fallback_chain() {
            if let Some(catalog) = catalogs.by_locale.get(&candidate) {
                if let Some(text) = catalog.format(key, args) {
                    return text;
                }
            }
        }

        // Nothing resolved anywhere. Render the key so the gap is visible in the
        // UI and greppable in a screenshot, rather than emitting an empty string
        // that silently deletes a button's label.
        tracing::warn!(i18n.key = key, i18n.locale = %locale, "missing translation");
        key.to_string()
    }

    fn has_key(&self, locale: &Locale, key: &str) -> bool {
        self.snapshot()
            .by_locale
            .get(locale)
            .is_some_and(|c| c.keys.contains(key))
    }

    fn available_locales(&self) -> Vec<Locale> {
        self.snapshot().order.clone()
    }

    fn default_locale_keys(&self) -> Vec<String> {
        self.snapshot().default_keys.clone()
    }

    async fn reload(&self) -> Result<(), AppError> {
        let roots = self.roots.clone();

        // Directory walking and parsing are blocking and can take tens of ms
        // across several locales; keep them off the async runtime.
        let catalogs = tokio::task::spawn_blocking(move || load_catalogs(&roots))
            .await
            .map_err(|e| AppError::internal(format!("catalog load task panicked: {e}")))?;

        let count = catalogs.order.len();
        match self.catalogs.write() {
            Ok(mut guard) => *guard = Arc::new(catalogs),
            Err(poisoned) => *poisoned.into_inner() = Arc::new(catalogs),
        }

        tracing::info!(i18n.locales = count, "translation catalogs loaded");
        Ok(())
    }
}

/// Reads every root and assembles one catalog per locale.
fn load_catalogs(roots: &[PathBuf]) -> Catalogs {
    // locale → sources, in ascending precedence order across roots.
    let mut sources: HashMap<Locale, Vec<String>> = HashMap::new();

    for root in roots {
        if !root.is_dir() {
            tracing::debug!(path = %root.display(), "locale root absent, skipping");
            continue;
        }
        for (locale, source) in read_root(root) {
            sources.entry(locale).or_default().push(source);
        }
    }

    let default_locale = Locale::default_locale();
    let mut by_locale = HashMap::new();

    for (locale, locale_sources) in sources {
        match build_bundle(&locale, &locale_sources) {
            Some(catalog) => {
                by_locale.insert(locale, catalog);
            }
            None => {
                tracing::error!(i18n.locale = %locale, "locale catalog failed to build, skipping");
            }
        }
    }

    // Default first, remainder alphabetical — a stable order so the admin
    // roster and the per-locale Tera build iterate predictably.
    let mut order: Vec<Locale> = by_locale.keys().cloned().collect();
    order.sort();
    if let Some(pos) = order.iter().position(|l| l == &default_locale) {
        let d = order.remove(pos);
        order.insert(0, d);
    }

    let default_keys = by_locale
        .get(&default_locale)
        .map(|c| {
            let mut k: Vec<String> = c.keys.iter().cloned().collect();
            k.sort();
            k
        })
        .unwrap_or_default();

    Catalogs {
        by_locale,
        order,
        default_keys,
    }
}

/// Reads one root directory, returning `(locale, merged_source)` per locale
/// subdirectory. Subdirectories whose name is not a valid locale tag are
/// skipped — this is the second line of defence behind `Locale::parse`.
fn read_root(root: &Path) -> Vec<(Locale, String)> {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(path = %root.display(), error = %e, "cannot read locale root");
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(locale) = Locale::parse(name) else {
            tracing::warn!(dir = name, "skipping locale directory with invalid tag");
            continue;
        };
        if let Some(source) = read_ftl_dir(&path) {
            out.push((locale, source));
        }
    }
    out
}

/// Concatenates every `.ftl` in a directory. Files are read in sorted order so
/// a duplicate key resolves the same way on every machine — directory iteration
/// order is not stable across filesystems.
fn read_ftl_dir(dir: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ftl"))
        .collect();
    files.sort();

    let mut merged = String::new();
    for file in files {
        match std::fs::read_to_string(&file) {
            Ok(content) => {
                merged.push_str(&content);
                // Guard against a file that does not end in a newline running
                // its last message into the next file's first one.
                merged.push('\n');
            }
            Err(e) => {
                tracing::warn!(path = %file.display(), error = %e, "cannot read ftl file");
            }
        }
    }

    (!merged.trim().is_empty()).then_some(merged)
}

/// Builds a bundle from sources in ascending precedence order.
fn build_bundle(locale: &Locale, sources: &[String]) -> Option<LocaleCatalog> {
    let lang_id: unic_langid::LanguageIdentifier = locale.as_str().parse().ok()?;
    let mut bundle = Bundle::new_concurrent(vec![lang_id]);

    // Fluent wraps placeables in Unicode bidi isolation marks (U+2068/U+2069) by
    // default. Those are correct for mixed LTR/RTL text but invisible and
    // surprising in LTR-only output — they break string equality in tests and
    // leak into `<title>` and meta tags. The locked decision is LTR-only, so
    // turn them off. Revisit if RTL is ever added.
    bundle.set_use_isolating(false);

    let mut keys = HashSet::new();

    for source in sources {
        keys.extend(message_ids(source));

        let resource = match FluentResource::try_new(source.clone()) {
            Ok(r) => r,
            Err((r, errors)) => {
                // Partial parse: Fluent returns the entries it did understand.
                // Keep them and report the rest rather than dropping a whole
                // locale over one malformed message.
                tracing::error!(
                    i18n.locale = %locale,
                    i18n.errors = ?errors,
                    "ftl parse errors; keeping successfully parsed messages"
                );
                r
            }
        };

        // `add_resource_overriding` rather than `add_resource`: a later root
        // (theme, plugin, admin override) is *meant* to shadow core, and the
        // non-overriding variant treats that as an error.
        bundle.add_resource_overriding(resource);
    }

    Some(LocaleCatalog { bundle, keys })
}

/// Extracts message ids from FTL source.
///
/// `FluentBundle` exposes no way to enumerate its messages, so the source is
/// parsed a second time with `fluent-syntax` purely to collect ids. This only
/// runs at load/reload, never on the render path.
fn message_ids(source: &str) -> HashSet<String> {
    let resource = match fluent_syntax::parser::parse(source) {
        Ok(r) => r,
        // Same partial-parse tolerance as above: take what parsed.
        Err((r, _errors)) => r,
    };

    resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            ast::Entry::Message(m) => Some(m.id.name.to_string()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

        fn load(&self) -> Catalogs {
            load_catalogs(&[self.0.clone()])
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn translator_over(roots: Vec<PathBuf>) -> FluentTranslator {
        FluentTranslator {
            catalogs: RwLock::new(Arc::new(load_catalogs(&roots))),
            roots,
        }
    }

    #[test]
    fn loads_and_formats_a_simple_message() {
        let f = Fixture::new("simple");
        f.write("en", "errors.ftl", "error-thread-locked = This thread is locked.\n");

        let t = translator_over(vec![f.0.clone()]);
        assert_eq!(
            t.translate(&Locale::default_locale(), "error-thread-locked", &[]),
            "This thread is locked."
        );
    }

    #[test]
    fn interpolates_arguments() {
        let f = Fixture::new("args");
        f.write("en", "c.ftl", "greet = Hello, { $name }!\n");

        let t = translator_over(vec![f.0.clone()]);
        assert_eq!(
            t.translate(
                &Locale::default_locale(),
                "greet",
                &[("name", TransArg::Str("Trung".into()))]
            ),
            "Hello, Trung!"
        );
    }

    #[test]
    fn selects_plural_form_from_a_numeric_argument() {
        // The reason TransArg distinguishes Int from Str: a string "2" would
        // fall through to the catch-all arm instead of matching `[one]`.
        let f = Fixture::new("plural");
        f.write(
            "en",
            "c.ftl",
            "replies = { $count ->\n    [one] 1 reply\n   *[other] { $count } replies\n  }\n",
        );

        let t = translator_over(vec![f.0.clone()]);
        let en = Locale::default_locale();
        assert_eq!(t.translate(&en, "replies", &[("count", TransArg::Int(1))]), "1 reply");
        assert_eq!(t.translate(&en, "replies", &[("count", TransArg::Int(5))]), "5 replies");
    }

    #[test]
    fn falls_back_to_default_locale_for_untranslated_keys() {
        let f = Fixture::new("fallback");
        f.write("en", "c.ftl", "only-in-en = English text\nshared = English shared\n");
        f.write("vi", "c.ftl", "shared = Tiếng Việt\n");

        let t = translator_over(vec![f.0.clone()]);
        let vi = Locale::parse("vi").unwrap();

        assert_eq!(t.translate(&vi, "shared", &[]), "Tiếng Việt");
        // Untranslated in vi → resolves through the chain to en.
        assert_eq!(t.translate(&vi, "only-in-en", &[]), "English text");
    }

    #[test]
    fn regional_locale_narrows_to_its_base_language() {
        let f = Fixture::new("regional");
        f.write("en", "c.ftl", "colour = color\nshared = base\n");
        f.write("en-GB", "c.ftl", "colour = colour\n");

        let t = translator_over(vec![f.0.clone()]);
        let gb = Locale::parse("en-GB").unwrap();

        assert_eq!(t.translate(&gb, "colour", &[]), "colour");
        // Not overridden regionally → falls back to `en`.
        assert_eq!(t.translate(&gb, "shared", &[]), "base");
    }

    #[test]
    fn missing_key_renders_the_key_rather_than_empty_string() {
        let f = Fixture::new("missing");
        f.write("en", "c.ftl", "present = yes\n");

        let t = translator_over(vec![f.0.clone()]);
        assert_eq!(
            t.translate(&Locale::default_locale(), "totally-absent", &[]),
            "totally-absent"
        );
    }

    #[test]
    fn later_root_overrides_earlier_one() {
        // Core ships a string; a theme shadows it. Same precedence direction as
        // the theme template chain.
        let core = Fixture::new("core");
        let theme = Fixture::new("theme");
        core.write("en", "c.ftl", "site-title = Ferum Board\ncore-only = kept\n");
        theme.write("en", "c.ftl", "site-title = My Community\n");

        let t = translator_over(vec![core.0.clone(), theme.0.clone()]);
        let en = Locale::default_locale();

        assert_eq!(t.translate(&en, "site-title", &[]), "My Community");
        assert_eq!(t.translate(&en, "core-only", &[]), "kept");
    }

    #[test]
    fn has_key_distinguishes_own_translation_from_inherited() {
        let f = Fixture::new("haskey");
        f.write("en", "c.ftl", "a = A\nb = B\n");
        f.write("vi", "c.ftl", "a = A-vi\n");

        let t = translator_over(vec![f.0.clone()]);
        let vi = Locale::parse("vi").unwrap();

        assert!(t.has_key(&vi, "a"));
        // Resolvable via fallback, but NOT translated in vi — coverage must
        // count this as missing or the admin percentage is meaningless.
        assert!(!t.has_key(&vi, "b"));
        assert_eq!(t.translate(&vi, "b", &[]), "B");
    }

    #[test]
    fn invalid_locale_directory_is_skipped() {
        let f = Fixture::new("badlocale");
        f.write("en", "c.ftl", "a = A\n");
        f.write("not-a-locale-at-all", "c.ftl", "a = bad\n");

        let catalogs = f.load();
        assert_eq!(catalogs.order, vec![Locale::default_locale()]);
    }

    #[test]
    fn default_locale_sorts_first_in_the_roster() {
        let f = Fixture::new("order");
        for l in ["ar", "en", "vi", "de"] {
            f.write(l, "c.ftl", "a = A\n");
        }
        let catalogs = f.load();
        assert_eq!(catalogs.order[0], Locale::default_locale());
        assert_eq!(catalogs.order.len(), 4);
    }

    #[test]
    fn malformed_message_does_not_discard_the_rest_of_the_file() {
        let f = Fixture::new("partial");
        f.write("en", "c.ftl", "good-one = fine\n= broken\ngood-two = also fine\n");

        let t = translator_over(vec![f.0.clone()]);
        let en = Locale::default_locale();
        assert_eq!(t.translate(&en, "good-one", &[]), "fine");
        assert_eq!(t.translate(&en, "good-two", &[]), "also fine");
    }

    #[test]
    fn no_bidi_isolation_marks_in_output() {
        // set_use_isolating(false) — otherwise every interpolated value is
        // wrapped in U+2068/U+2069 and string comparisons mysteriously fail.
        let f = Fixture::new("isolating");
        f.write("en", "c.ftl", "hi = Hi { $name }\n");

        let t = translator_over(vec![f.0.clone()]);
        let out = t.translate(&Locale::default_locale(), "hi", &[("name", "Bo".into())]);

        assert_eq!(out, "Hi Bo");
        assert!(!out.contains('\u{2068}'), "unexpected bidi isolate in {out:?}");
    }
}
