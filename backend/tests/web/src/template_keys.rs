//! Verifies that every `t()` key used by a template actually resolves.
//!
//! This is the check that would have caught the whole navigation rendering
//! `ui-home` / `ui-categories`: the catalogs were fine and every other test
//! passed, but nothing tied the keys the *templates ask for* to the keys the
//! *catalogs define*. A missing entry is invisible until someone looks at the
//! page, because `translate` deliberately falls back to rendering the key.
//!
//! Scanning the templates rather than maintaining a list is on purpose — a list
//! would need the same discipline this test exists to replace.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ferum_application::ports::Translator;
use ferum_domain::Locale;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// The same roots `startup.rs` assembles: core first, then every theme's own
/// catalog. A theme-provided key must count as resolvable.
async fn translator() -> impl Translator {
    let mut roots = vec![repo_root().join("locales")];
    let themes = repo_root().join("frontend").join("themes");
    if let Ok(entries) = std::fs::read_dir(&themes) {
        let mut theme_roots: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("locales"))
            .filter(|p| p.is_dir())
            .collect();
        theme_roots.sort();
        roots.extend(theme_roots);
    }
    ferum_infrastructure::i18n::FluentTranslator::new(roots).await
}

fn html_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "node_modules") {
                continue;
            }
            html_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "html") {
            out.push(path);
        }
    }
}

/// `key -> files that use it`, across every template the app can render.
fn keys_used_by_templates() -> BTreeMap<String, BTreeSet<String>> {
    let frontend = repo_root().join("frontend");
    let mut files = Vec::new();
    html_files(&frontend.join("themes"), &mut files);
    html_files(&frontend.join("templates"), &mut files);

    let mut used: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = file
            .strip_prefix(&frontend)
            .unwrap_or(&file)
            .display()
            .to_string()
            .replace('\\', "/");

        // Matches both quoting styles the templates use:
        //   {{ t(k="ui-home") }}   and   title="{{ t(k='ui-home') }}"
        for (marker, close) in [("t(k=\"", '"'), ("t(k='", '\'')] {
            let mut rest = src.as_str();
            while let Some(idx) = rest.find(marker) {
                rest = &rest[idx + marker.len()..];
                let Some(end) = rest.find(close) else { continue };
                let key = &rest[..end];

                // A key built by concatenation — `t(k="language-name-" ~ alt)` —
                // has no single literal to check, and the prefix alone is not a
                // real key. The switcher builds one per installed locale, and
                // `every_installed_locale_names_itself` covers those instead.
                let is_concatenated = rest[end + 1..].trim_start().starts_with('~');

                if !key.is_empty() && !key.contains(char::is_whitespace) && !is_concatenated {
                    used.entry(key.to_string()).or_default().insert(rel.clone());
                }
            }
        }
    }
    used
}

#[tokio::test]
async fn every_template_key_resolves_in_the_default_catalog() {
    let t = translator().await;
    let en = Locale::default_locale();
    let used = keys_used_by_templates();

    assert!(
        used.len() > 500,
        "only {} keys found across the templates — the scanner is broken, which \
         would make this test vacuous",
        used.len()
    );

    let missing: Vec<String> = used
        .iter()
        .filter(|(key, _)| !t.has_key(&en, key))
        .map(|(key, files)| {
            format!(
                "{key}\n      used in: {}",
                files.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        })
        .collect();

    assert!(
        missing.is_empty(),
        "{} template key(s) have no catalog entry, so these pages render the raw \
         key to users:\n    {}",
        missing.len(),
        missing.join("\n    ")
    );
}

#[tokio::test]
async fn no_catalog_key_is_defined_but_unused() {
    // The other direction. Not fatal — shared keys legitimately outlive a single
    // template, and `js-*` keys are used from JavaScript, not markup — so this
    // only reports, keeping the catalogs from silently accreting dead strings.
    let t = translator().await;
    let used = keys_used_by_templates();

    let orphans: Vec<String> = t
        .default_locale_keys()
        .into_iter()
        // `js-*` are consumed by Ferum.t() in the browser; `error-*` by the
        // error middleware; `format-*`/`month-short-*` by the date filter.
        .filter(|k| {
            !k.starts_with("js-")
                && !k.starts_with("error-")
                && !k.starts_with("format-")
                && !k.starts_with("month-short-")
                && !k.starts_with("language-name-")
                && !k.starts_with("email-")
        })
        .filter(|k| !used.contains_key(k))
        .collect();

    if !orphans.is_empty() {
        eprintln!(
            "note: {} catalog key(s) are not referenced by any template:\n  {}",
            orphans.len(),
            orphans.join("\n  ")
        );
    }
}
