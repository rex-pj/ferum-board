use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use ferum_application::ports::{TransArg, Translator};
use ferum_domain::Locale;
use tera::{Context, Tera};
use tokio::sync::RwLock;

/// One compiled template set per locale.
///
/// Tera 1.x global functions receive their arguments but **cannot read the
/// render context**, so a single shared `Tera` has no way to know which locale
/// the current request wants. The options were:
///
/// * Pass the locale at every call site — `{{ t(k="x", loc=locale) }}` — which
///   puts noise on ~960 strings and is silently wrong if one is forgotten.
/// * Read it from a `tokio::task_local!` — clean at the call site, but breaks
///   the moment `render` moves onto `spawn_blocking`, which it already does.
/// * Build one `Tera` per locale, each closing over its own locale.
///
/// The last is chosen. Templates just write `{{ t(k="thread-reply") }}`, and the
/// cost is one extra parse of ~10k lines of HTML per locale — a few MB for the
/// two-to-four locales this product targets. Rebuilds already happen off-thread
/// behind an atomic swap, so the multiplier never touches the request path.
type Instances = HashMap<Locale, Arc<Tera>>;

#[derive(Clone)]
pub struct TeraEngine {
    instances: Arc<RwLock<Arc<Instances>>>,
    themes_dir: PathBuf,
    admin_templates_dir: PathBuf,
    static_dir: PathBuf,
    translator: Arc<dyn Translator>,
}

impl TeraEngine {
    pub fn new(
        themes_dir: PathBuf,
        admin_templates_dir: PathBuf,
        static_dir: PathBuf,
        translator: Arc<dyn Translator>,
    ) -> Result<Self> {
        let instances =
            Self::build_all(&themes_dir, &admin_templates_dir, &static_dir, &translator)?;
        Ok(Self {
            instances: Arc::new(RwLock::new(Arc::new(instances))),
            themes_dir,
            admin_templates_dir,
            static_dir,
            translator,
        })
    }

    /// Builds one template set per installed locale.
    ///
    /// The default locale is always built, even when no catalog exists on disk,
    /// so the site still renders (with keys showing through) rather than having
    /// no templates at all.
    fn build_all(
        themes_dir: &PathBuf,
        admin_templates_dir: &PathBuf,
        static_dir: &PathBuf,
        translator: &Arc<dyn Translator>,
    ) -> Result<Instances> {
        let mut locales = translator.available_locales();
        let default = Locale::default_locale();
        if !locales.contains(&default) {
            locales.insert(0, default);
        }

        let mut instances = HashMap::new();
        for locale in locales {
            let tera = Self::build_tera(
                themes_dir,
                admin_templates_dir,
                static_dir,
                &locale,
                translator,
            )?;
            instances.insert(locale, Arc::new(tera));
        }
        Ok(instances)
    }

    /// Loads every template into one `Tera`, with two different failure policies:
    ///
    /// * **First-party templates fail closed.** `frontend/templates/` (admin, mod,
    ///   setup) and the built-in `default` theme ship with the binary. If one of
    ///   them will not parse the app refuses to start, because the alternative is
    ///   a 500 on whichever page happens to use it — silently, at first render.
    ///
    /// * **User-installed themes fail open.** An admin can upload an arbitrary
    ///   `.zip`; a typo in it must not take the forum down. A broken theme is
    ///   logged and skipped wholesale, and `render_with_theme`'s inheritance chain
    ///   falls back to `default`.
    ///
    /// Each third-party theme is trial-loaded into a clone and only committed if
    /// it parses. Tera's `add_raw_templates` is a batch that aborts on the first
    /// bad template, so loading every theme together would let one bad upload stop
    /// later themes — including `default` — from registering at all.
    fn build_tera(
        themes_dir: &PathBuf,
        admin_templates_dir: &PathBuf,
        static_dir: &PathBuf,
        locale: &Locale,
        translator: &Arc<dyn Translator>,
    ) -> Result<Tera> {
        // Read content ourselves rather than using Tera::new()'s glob: on Windows a
        // glob yields names like "admin\dashboard.html", but handlers address
        // templates with forward slashes.
        let mut tera = Tera::default();

        // ── First-party: built-in `default` theme ────────────────────────────
        let (default_theme, user_themes) = partition_theme_files(themes_dir);
        if default_theme.is_empty() {
            tracing::warn!("No default-theme templates found at: {}", themes_dir.display());
        } else {
            let raw = read_all(&default_theme);
            let pairs: Vec<(&str, &str)> = raw.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();
            tera.add_raw_templates(pairs).map_err(|e| {
                anyhow::anyhow!("built-in `default` theme failed to parse: {}", error_chain(&e))
            })?;
            tracing::info!("Default theme templates loaded: {} files", raw.len());
        }

        // ── First-party: admin / mod / setup templates ───────────────────────
        let admin_files = collect_html_templates(admin_templates_dir);
        if admin_files.is_empty() {
            tracing::warn!("No admin templates found at: {}", admin_templates_dir.display());
        } else {
            let mut failures: Vec<String> = Vec::new();
            let mut loaded = 0usize;
            for (path, name_opt) in &admin_files {
                let Some(name) = name_opt else { continue };
                match tera.add_template_files(vec![(path.clone(), Some(name.clone()))]) {
                    Ok(()) => loaded += 1,
                    Err(e) => failures.push(format!("{}: {}", name, error_chain(&e))),
                }
            }
            if !failures.is_empty() {
                return Err(anyhow::anyhow!(
                    "{} admin/mod template(s) failed to parse:\n  - {}",
                    failures.len(),
                    failures.join("\n  - ")
                ));
            }
            tracing::info!("Admin templates loaded: {} files", loaded);
        }

        // ── Third-party: user-installed themes, one isolated batch each ──────
        //
        // A theme may `{% extends %}` another theme, not just `default`, and slug
        // order says nothing about that dependency. So retry in passes: a theme
        // that failed only because its parent was not loaded yet succeeds on a
        // later pass. When a whole pass loads nothing, the survivors are genuinely
        // broken (bad syntax, or a parent that does not exist).
        let mut pending: Vec<(String, ThemeFiles)> = user_themes;
        while !pending.is_empty() {
            let mut deferred: Vec<(String, ThemeFiles)> = Vec::new();
            let mut progressed = false;

            for (slug, files) in pending {
                let raw = read_all(&files);
                let pairs: Vec<(&str, &str)> =
                    raw.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();

                let mut candidate = tera.clone();
                match candidate.add_raw_templates(pairs) {
                    Ok(()) => {
                        // Commit only on success — a partially-registered broken
                        // theme never reaches the live Tera instance.
                        tera = candidate;
                        progressed = true;
                        tracing::info!("Theme '{}' loaded: {} files", slug, raw.len());
                    }
                    Err(e) => {
                        // Might just be an unloaded parent; retry next pass. The
                        // error is only worth reporting once no pass can progress.
                        tracing::debug!("Theme '{}' deferred: {}", slug, error_chain(&e));
                        deferred.push((slug, files));
                    }
                }
            }

            if !progressed {
                for (slug, files) in &deferred {
                    let raw = read_all(files);
                    let pairs: Vec<(&str, &str)> =
                        raw.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();
                    let err = tera
                        .clone()
                        .add_raw_templates(pairs)
                        .err()
                        .map(|e| error_chain(&e))
                        .unwrap_or_else(|| "unknown error".into());
                    tracing::error!("Theme '{}' skipped, it failed to load: {}", slug, err);
                }
                break;
            }
            pending = deferred;
        }

        // Register the `asset_version()` global so templates can cache-bust static
        // assets. The token is derived once — at load / reload time — from the newest
        // mtime across the served JS, CSS and theme assets. When any of them changes
        // the URL `?v=<token>` changes and browsers fetch the fresh file instead of a
        // stale cached copy. Without this, `Cache-Control: max-age=86400` on /static
        // means client-side fixes can take up to a day to reach returning users.
        let token = compute_asset_version(static_dir, themes_dir);
        tera.register_function(
            "asset_version",
            move |_args: &std::collections::HashMap<String, tera::Value>| {
                Ok(tera::Value::String(token.clone()))
            },
        );

        // `t(k="key", ...)` — resolves a message from the translation catalog in
        // *this instance's* locale, which is why the engine keeps one Tera per
        // locale rather than one shared instance.
        //
        // Any argument other than `k` is passed through to the catalog as a
        // translation variable, so `{{ t(k="thread-replies", count=n) }}` selects
        // the right plural form. Numbers must stay numbers here: handing Fluent a
        // stringified count collapses every plural rule to its catch-all arm.
        //
        // Output is a plain string and is escaped by Tera like any other value.
        // Translations must never be piped through `| safe` — catalogs are
        // admin-editable, so that would turn a translation into an XSS vector.
        let t_locale = locale.clone();
        let t_translator = Arc::clone(translator);
        tera.register_function(
            "t",
            move |args: &std::collections::HashMap<String, tera::Value>| {
                let key = args
                    .get("k")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| tera::Error::msg("t() requires a `k` argument naming the message key"))?;

                let trans_args: Vec<(&str, TransArg)> = args
                    .iter()
                    .filter(|(name, _)| name.as_str() != "k")
                    .filter_map(|(name, value)| {
                        let arg = match value {
                            tera::Value::String(s) => TransArg::Str(s.clone()),
                            tera::Value::Number(n) => n
                                .as_i64()
                                .map(TransArg::Int)
                                .or_else(|| n.as_f64().map(TransArg::Float))?,
                            tera::Value::Bool(b) => TransArg::Str(b.to_string()),
                            // Null/array/object have no sensible Fluent mapping;
                            // dropping them lets the catalog's own fallback show
                            // rather than rendering "[object]" into the page.
                            _ => return None,
                        };
                        Some((name.as_str(), arg))
                    })
                    .collect();

                Ok(tera::Value::String(t_translator.translate(
                    &t_locale,
                    key,
                    trans_args.as_slice(),
                )))
            },
        );

        // `thousands` filter — groups an integer for readability:
        // 15000000 → "15,000,000" in English, "15.000.000" in a locale whose
        // catalog says so. The separator comes from the catalog rather than a
        // constant because it is genuinely a language decision, not a style one.
        // Non-numeric input passes through unchanged so a template never errors
        // on a missing price.
        let sep_locale = locale.clone();
        let sep_translator = Arc::clone(translator);
        tera.register_filter(
            "thousands",
            move |value: &tera::Value, _args: &std::collections::HashMap<String, tera::Value>| {
                match value.as_i64().or_else(|| value.as_f64().map(|f| f as i64)) {
                    Some(n) => {
                        let sep =
                            sep_translator.translate(&sep_locale, "format-thousands-separator", &[]);
                        // A catalog that omits the key resolves to the key name;
                        // fall back to a comma rather than splicing that in.
                        let sep = if sep.len() == 1 { sep } else { ",".to_string() };
                        Ok(tera::Value::String(group_thousands(n, &sep)))
                    }
                    None => Ok(value.clone()),
                }
            },
        );

        // `localdate` filter — renders an RFC 3339 timestamp using catalog-owned
        // month names and field order.
        //
        // `date(format="%b %d, %Y")` cannot be localized: chrono's `%b` is always
        // English, and the field order is baked into the format string even
        // though languages disagree about it (English "Jan 5, 2026" vs Vietnamese
        // "5 thg 1, 2026"). Both the month name and the arrangement therefore
        // live in the catalog, and this filter only supplies the numbers.
        //
        // Usage: {{ ts | localdate }} or {{ ts | localdate(style="datetime") }}
        let date_locale = locale.clone();
        let date_translator = Arc::clone(translator);
        tera.register_filter(
            "localdate",
            move |value: &tera::Value, args: &std::collections::HashMap<String, tera::Value>| {
                let Some(raw) = value.as_str() else {
                    return Ok(value.clone());
                };
                let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) else {
                    // Not a timestamp we understand — pass through rather than
                    // failing the whole page render over one field.
                    return Ok(value.clone());
                };

                let style = args
                    .get("style")
                    .and_then(|v| v.as_str())
                    .unwrap_or("date");

                use chrono::{Datelike, Timelike};
                let month = date_translator.translate(
                    &date_locale,
                    &format!("month-short-{}", dt.month()),
                    &[],
                );
                let rendered = date_translator.translate(
                    &date_locale,
                    &format!("format-{style}"),
                    &[
                        ("day", TransArg::Int(dt.day() as i64)),
                        ("month", TransArg::Str(month)),
                        ("year", TransArg::Int(dt.year() as i64)),
                        // Time fields are pre-padded strings: Fluent would format
                        // a bare number as "9", never "09".
                        ("hour", TransArg::Str(format!("{:02}", dt.hour()))),
                        ("minute", TransArg::Str(format!("{:02}", dt.minute()))),
                    ],
                );
                Ok(tera::Value::String(rendered))
            },
        );

        Ok(tera)
    }

    /// Reload theme templates from disk without restarting. Call after a theme
    /// upload, or after a language pack changes the set of installed locales.
    pub async fn reload_themes(&self) -> Result<()> {
        let themes_dir = self.themes_dir.clone();
        let admin_dir = self.admin_templates_dir.clone();
        let static_dir = self.static_dir.clone();
        let translator = Arc::clone(&self.translator);
        // build_all does blocking file I/O + CPU-bound compilation, now once per
        // locale — must not run on the async worker thread or it will stall all
        // concurrent HTTP requests.
        let fresh = tokio::task::spawn_blocking(move || {
            Self::build_all(&themes_dir, &admin_dir, &static_dir, &translator)
        })
        .await
        .map_err(|e| anyhow::anyhow!("build_tera join error: {e}"))??;
        let count = fresh.len();
        *self.instances.write().await = Arc::new(fresh);
        tracing::info!(locales = count, "Tera templates reloaded from disk");
        Ok(())
    }

    /// Resolves the template set for `locale`, falling back to the default.
    ///
    /// A locale with a catalog but somehow no compiled instance must not 500 the
    /// page; serving the default language is the correct degradation.
    async fn instance_for(&self, locale: &Locale) -> Arc<Tera> {
        let instances = Arc::clone(&*self.instances.read().await);
        if let Some(tera) = instances.get(locale) {
            return Arc::clone(tera);
        }
        if let Some(tera) = instances.get(&Locale::default_locale()) {
            return Arc::clone(tera);
        }
        // Only reachable if build_all produced nothing, which `new` treats as an
        // error — so this is a defensive default rather than a real path.
        Arc::new(Tera::default())
    }

    /// Return the first candidate template name that exists, holding only one lock.
    /// Used by render_with_theme() to walk the inheritance chain without multiple async round-trips.
    ///
    /// Every locale compiles the *same* template files — only the `t()` binding
    /// differs — so existence can be answered from the default instance alone.
    pub async fn first_existing_template(&self, candidates: &[String]) -> Option<String> {
        let tera = self.instance_for(&Locale::default_locale()).await;
        candidates
            .iter()
            .find(|n| tera.get_template(n).is_ok())
            .cloned()
    }

    /// Render a template by name, in `locale`, with the given context.
    ///
    /// The locale selects which compiled instance runs — and therefore which
    /// catalog `t()` resolves against. Note the render itself happens on
    /// `spawn_blocking` below: that is precisely why the locale is carried in
    /// the instance rather than in a task-local, which would not survive the
    /// hop off the async task.
    pub async fn render(
        &self,
        locale: &Locale,
        template_name: &str,
        ctx: &Context,
    ) -> Result<String> {
        // Clone the Arc<Tera> (one atomic increment) while holding the read lock briefly,
        // then release the lock before the CPU-bound render. This avoids copying all
        // compiled template ASTs on every request, which was the previous bottleneck.
        let lock_start = std::time::Instant::now();
        let tera = self.instance_for(locale).await;
        let lock_us = lock_start.elapsed().as_micros() as u64;

        let name = template_name.to_string();
        let ctx = ctx.clone();
        let name2 = name.clone();

        let render_start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || tera.render(&name2, &ctx))
            .await
            .map_err(|e| anyhow::anyhow!("render task join error: {e}"))?
            .map_err(|e| {
                let mut chain = format!("Tera render error for '{}': {}", name, e);
                let mut src: &dyn std::error::Error = &e;
                while let Some(cause) = src.source() {
                    chain.push_str(&format!("\n  caused by: {}", cause));
                    src = cause;
                }
                anyhow::anyhow!("{}", chain)
            });

        tracing::debug!(
            template = template_name,
            lock_us,
            render_us = render_start.elapsed().as_micros() as u64,
            ok = result.is_ok(),
            "tera_render"
        );

        result
    }
}

/// Derive a cache-busting token from the newest mtime across every client asset
/// the app serves: all of `static/js` and `static/css`, plus each theme's
/// `assets/` directory. Returns seconds-since-epoch (stringified); falls back to
/// "dev" when nothing can be read so templates still render a valid URL.
///
/// The directories are scanned rather than listed. An earlier version named
/// seven JS files explicitly, which meant editing anything outside that list —
/// every `ferum-page-*.js`, `ferum-admin-products.js`, or any theme stylesheet —
/// left the token unchanged and browsers kept serving the stale copy that
/// `Cache-Control: max-age=86400` had pinned for up to a day.
fn compute_asset_version(static_dir: &PathBuf, themes_dir: &PathBuf) -> String {
    /// Newest mtime among the immediate files of `dir`, as seconds since epoch.
    fn newest_in(dir: &std::path::Path) -> Option<u64> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(|entry| {
                let meta = entry.ok()?.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                meta.modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            })
            .max()
    }

    let mut newest = [static_dir.join("js"), static_dir.join("css")]
        .iter()
        .filter_map(|d| newest_in(d))
        .max();

    // Each installed theme keeps its stylesheet in `<slug>/assets/`.
    if let Ok(entries) = std::fs::read_dir(themes_dir) {
        for entry in entries.flatten() {
            let assets = entry.path().join("assets");
            if let Some(secs) = newest_in(&assets) {
                newest = Some(newest.map_or(secs, |cur: u64| cur.max(secs)));
            }
        }
    }

    newest
        .map(|secs| secs.to_string())
        .unwrap_or_else(|| "dev".to_string())
}

/// Group an integer with comma thousands-separators: 15000000 → "15,000,000".
/// Negatives keep their sign.
fn group_thousands(n: i64, sep: &str) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    let bytes = digits.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push_str(sep);
        }
        out.push(*b as char);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// Flatten a `std::error::Error` chain into one line — Tera nests the actual
/// parse message (line/column, unexpected token) inside `source()`, so logging
/// only the top-level error tells you a template failed but never why.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut chain = e.to_string();
    let mut src = e.source();
    while let Some(cause) = src {
        chain.push_str(&format!(" -> {}", cause));
        src = cause.source();
    }
    chain
}

/// Read each `(path, name)` pair into `(name, content)`. Unreadable files are
/// logged and skipped: an I/O error on one file should not be reported as a
/// parse failure of the whole theme.
fn read_all(files: &[(PathBuf, Option<String>)]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(files.len());
    for (path, name_opt) in files {
        let Some(name) = name_opt else { continue };
        match std::fs::read_to_string(path) {
            Ok(content) => out.push((name.clone(), content)),
            Err(e) => tracing::warn!("Failed to read template {:?}: {}", path, e),
        }
    }
    out
}

/// Split `themes_dir` into the built-in `default` theme's files and the files of
/// each user-installed theme, keyed by slug. Template names are theme-relative
/// (`"{slug}/templates/{page}"`), which is exactly what `render_with_theme`
/// builds its candidate list from, so the slug is the name's first path segment.
type ThemeFiles = Vec<(PathBuf, Option<String>)>;
fn partition_theme_files(themes_dir: &PathBuf) -> (ThemeFiles, Vec<(String, ThemeFiles)>) {
    use std::collections::BTreeMap;

    let mut default_theme: ThemeFiles = Vec::new();
    // BTreeMap keeps load order deterministic across runs and platforms.
    let mut by_slug: BTreeMap<String, ThemeFiles> = BTreeMap::new();

    for (path, name_opt) in collect_html_templates(themes_dir) {
        let Some(name) = name_opt.clone() else { continue };
        let slug = name.split('/').next().unwrap_or_default().to_string();
        if slug == "default" {
            default_theme.push((path, name_opt));
        } else {
            by_slug.entry(slug).or_default().push((path, name_opt));
        }
    }

    (default_theme, by_slug.into_iter().collect())
}

/// Walk `dir` recursively and return `(path, Some(name))` pairs for all `.html` files.
/// Names always use forward slashes regardless of OS path separator.
fn collect_html_templates(dir: &PathBuf) -> Vec<(PathBuf, Option<String>)> {
    let mut out = Vec::new();
    if dir.exists() {
        walk_html_dir(dir, dir, &mut out);
    }
    out
}

fn walk_html_dir(base: &PathBuf, current: &PathBuf, out: &mut Vec<(PathBuf, Option<String>)>) {
    let Ok(entries) = std::fs::read_dir(current) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_html_dir(base, &path, out);
        } else if path.extension().map_or(false, |e| e == "html") {
            if let Ok(rel) = path.strip_prefix(base) {
                let name = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((path, Some(name)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders `<locale>:<key>(+args)` so a test can assert both that `t()` was
    /// wired and *which* locale's instance ran, without needing real catalogs.
    struct StubTranslator;

    #[async_trait::async_trait]
    impl Translator for StubTranslator {
        fn translate(&self, locale: &Locale, key: &str, args: &[(&str, TransArg)]) -> String {
            let mut pairs: Vec<String> = args
                .iter()
                .map(|(name, value)| {
                    // Rendered without quotes so the assertion is not entangled
                    // with Tera's HTML escaping of `"`.
                    let rendered = match value {
                        TransArg::Str(s) => format!("Str({s})"),
                        TransArg::Int(i) => format!("Int({i})"),
                        TransArg::Float(f) => format!("Float({f})"),
                    };
                    format!("|{name}={rendered}")
                })
                .collect();
            pairs.sort();
            format!("{locale}:{key}{}", pairs.concat())
        }
        fn has_key(&self, _locale: &Locale, _key: &str) -> bool {
            true
        }
        fn available_locales(&self) -> Vec<Locale> {
            vec![Locale::default_locale()]
        }
        fn default_locale_keys(&self) -> Vec<String> {
            Vec::new()
        }
        async fn reload(&self) -> std::result::Result<(), ferum_application::shared::AppError> {
            Ok(())
        }
    }

    /// Scratch tree: `<tmp>/<label>-<nonce>/{themes,templates,static}`.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static N: AtomicUsize = AtomicUsize::new(0);
            let nonce = format!(
                "{}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed),
                label
            );
            let root = std::env::temp_dir().join(format!("ferum-tera-{nonce}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("themes")).unwrap();
            std::fs::create_dir_all(root.join("templates")).unwrap();
            std::fs::create_dir_all(root.join("static")).unwrap();
            Self(root)
        }

        fn write(&self, rel: &str, body: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }

        fn build(&self) -> Result<Tera> {
            self.build_in(&Locale::default_locale())
        }

        fn build_in(&self, locale: &Locale) -> Result<Tera> {
            let translator: Arc<dyn Translator> = Arc::new(StubTranslator);
            TeraEngine::build_tera(
                &self.0.join("themes"),
                &self.0.join("templates"),
                &self.0.join("static"),
                locale,
                &translator,
            )
        }

        /// Minimal well-formed `default` theme; every test needs one.
        fn with_default_theme(self) -> Self {
            self.write("themes/default/templates/base.html", "<html>{% block content %}{% endblock %}</html>");
            self.write("themes/default/templates/home.html", "{% extends \"default/templates/base.html\" %}");
            self
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn healthy_tree_loads_everything() {
        let s = Scratch::new("healthy").with_default_theme();
        s.write("templates/admin/base.html", "<html>{% block b %}{% endblock %}</html>");

        let tera = s.build().expect("a healthy tree must build");
        assert!(tera.get_template("default/templates/home.html").is_ok());
        assert!(tera.get_template("admin/base.html").is_ok());
    }

    /// First-party admin/mod templates fail CLOSED: the app must refuse to boot
    /// rather than serve a 500 from a page nobody noticed was broken.
    #[test]
    fn broken_admin_template_is_a_hard_error() {
        let s = Scratch::new("bad-admin").with_default_theme();
        s.write("templates/admin/broken.html", "{% if x %}never closed");

        let err = s.build().expect_err("a broken admin template must abort the build");
        let msg = err.to_string();
        assert!(msg.contains("admin/broken.html"), "error must name the file, got: {msg}");
    }

    /// The built-in `default` theme is first-party too, and nothing can fall back
    /// to it, so it fails CLOSED as well.
    #[test]
    fn broken_default_theme_is_a_hard_error() {
        let s = Scratch::new("bad-default");
        s.write("themes/default/templates/base.html", "{% for a in b %}unterminated");

        let err = s.build().expect_err("a broken default theme must abort the build");
        assert!(
            err.to_string().contains("default"),
            "error must mention the default theme, got: {err}"
        );
    }

    /// A user-uploaded theme fails OPEN: it is skipped, the app still boots, and
    /// crucially the healthy themes around it stay registered.
    #[test]
    fn broken_user_theme_is_skipped_without_taking_down_the_site() {
        let s = Scratch::new("bad-user-theme").with_default_theme();
        s.write("templates/admin/base.html", "<html></html>");
        s.write("themes/aaa-broken/templates/base.html", "{% if nope %}unterminated");
        s.write("themes/zzz-good/templates/base.html", "<html>good</html>");

        let tera = s.build().expect("a broken user theme must not abort the build");

        assert!(
            tera.get_template("aaa-broken/templates/base.html").is_err(),
            "the broken theme must not be registered, not even partially"
        );
        // `aaa-broken` sorts before `zzz-good`: with a single shared batch the bad
        // theme used to abort the load and take this one down with it.
        assert!(
            tera.get_template("zzz-good/templates/base.html").is_ok(),
            "a healthy theme must survive a broken sibling"
        );
        assert!(
            tera.get_template("default/templates/home.html").is_ok(),
            "the default theme must survive a broken user theme"
        );
    }

    /// Themes may extend other themes; slug order says nothing about that, so the
    /// loader retries until it stops making progress.
    #[test]
    fn child_theme_loads_even_when_it_sorts_before_its_parent() {
        let s = Scratch::new("child-first").with_default_theme();
        // "aaa-child" is visited before "zzz-parent" it extends.
        s.write("themes/zzz-parent/templates/base.html", "<html>{% block c %}{% endblock %}</html>");
        s.write(
            "themes/aaa-child/templates/base.html",
            "{% extends \"zzz-parent/templates/base.html\" %}{% block c %}hi{% endblock %}",
        );

        let tera = s.build().expect("build must succeed");
        assert!(tera.get_template("zzz-parent/templates/base.html").is_ok());
        assert!(
            tera.get_template("aaa-child/templates/base.html").is_ok(),
            "child theme must load on a later pass once its parent is present"
        );
    }

    /// A theme extending a parent that does not exist can never load; the retry
    /// loop must terminate and skip it rather than spin.
    #[test]
    fn theme_with_missing_parent_is_skipped_and_loop_terminates() {
        let s = Scratch::new("missing-parent").with_default_theme();
        s.write(
            "themes/orphan/templates/base.html",
            "{% extends \"ghost/templates/base.html\" %}",
        );

        let tera = s.build().expect("build must succeed");
        assert!(tera.get_template("orphan/templates/base.html").is_err());
        assert!(tera.get_template("default/templates/home.html").is_ok());
    }

    /// The real repo tree must build. Guards the paths in the `frontend/` layout.
    #[test]
    fn repository_templates_build() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../frontend");
        let translator: Arc<dyn Translator> = Arc::new(StubTranslator);
        TeraEngine::build_tera(
            &root.join("themes"),
            &root.join("templates"),
            &root.join("static"),
            &Locale::default_locale(),
            &translator,
        )
        .expect("the checked-in templates must parse");
    }

    #[test]
    fn t_function_is_available_to_templates() {
        let s = Scratch::new("t-fn").with_default_theme();
        s.write("themes/default/templates/p.html", r#"{{ t(k="hello-world") }}"#);

        let tera = s.build().unwrap();
        let out = tera
            .render("default/templates/p.html", &Context::new())
            .unwrap();
        assert_eq!(out, "en:hello-world");
    }

    #[test]
    fn t_binds_the_locale_of_its_own_instance() {
        // The core of the per-locale design: the same template text resolves
        // against a different catalog depending on which instance renders it.
        let s = Scratch::new("t-locale").with_default_theme();
        s.write("themes/default/templates/p.html", r#"{{ t(k="greeting") }}"#);

        let vi = Locale::parse("vi").unwrap();
        let out = s
            .build_in(&vi)
            .unwrap()
            .render("default/templates/p.html", &Context::new())
            .unwrap();
        assert_eq!(out, "vi:greeting");
    }

    #[test]
    fn t_forwards_extra_arguments_and_keeps_numbers_numeric() {
        // A stringified count would collapse every Fluent plural rule to its
        // catch-all arm, so the Int/Str distinction has to survive the Tera hop.
        let s = Scratch::new("t-args").with_default_theme();
        s.write(
            "themes/default/templates/p.html",
            r#"{{ t(k="replies", count=5, who="bo") }}"#,
        );

        let out = s
            .build()
            .unwrap()
            .render("default/templates/p.html", &Context::new())
            .unwrap();
        assert_eq!(out, "en:replies|count=Int(5)|who=Str(bo)");
    }

    #[test]
    fn t_without_a_key_is_a_render_error_not_a_panic() {
        let s = Scratch::new("t-nokey").with_default_theme();
        s.write("themes/default/templates/p.html", "{{ t() }}");

        let tera = s.build().unwrap();
        assert!(tera
            .render("default/templates/p.html", &Context::new())
            .is_err());
    }

    /// Resolves format keys the way a real catalog would, so the date/number
    /// filters can be tested without shipping fixtures.
    struct FormatTranslator;

    #[async_trait::async_trait]
    impl Translator for FormatTranslator {
        fn translate(&self, _: &Locale, key: &str, args: &[(&str, TransArg)]) -> String {
            let get = |name: &str| {
                args.iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| match v {
                        TransArg::Str(s) => s.clone(),
                        TransArg::Int(i) => i.to_string(),
                        TransArg::Float(f) => f.to_string(),
                    })
                    .unwrap_or_default()
            };
            match key {
                "format-date" => format!("{} {}, {}", get("month"), get("day"), get("year")),
                "format-datetime" => format!(
                    "{} {}, {} {}:{}",
                    get("month"),
                    get("day"),
                    get("year"),
                    get("hour"),
                    get("minute")
                ),
                "format-thousands-separator" => ".".to_string(),
                k if k.starts_with("month-short-") => {
                    format!("M{}", k.trim_start_matches("month-short-"))
                }
                other => other.to_string(),
            }
        }
        fn has_key(&self, _: &Locale, _: &str) -> bool {
            true
        }
        fn available_locales(&self) -> Vec<Locale> {
            vec![Locale::default_locale()]
        }
        fn default_locale_keys(&self) -> Vec<String> {
            Vec::new()
        }
        async fn reload(&self) -> std::result::Result<(), ferum_application::shared::AppError> {
            Ok(())
        }
    }

    fn render_with(translator: Arc<dyn Translator>, body: &str) -> String {
        let s = Scratch::new("fmt").with_default_theme();
        s.write("themes/default/templates/p.html", body);
        let tera = TeraEngine::build_tera(
            &s.0.join("themes"),
            &s.0.join("templates"),
            &s.0.join("static"),
            &Locale::default_locale(),
            &translator,
        )
        .unwrap();
        tera.render("default/templates/p.html", &Context::new())
            .unwrap()
    }

    #[test]
    fn localdate_uses_catalog_month_names_and_field_order() {
        // The whole reason `date(format="%b %d, %Y")` had to go: chrono's month
        // names are always English and the field order is fixed in the pattern.
        let out = render_with(
            Arc::new(FormatTranslator),
            r#"{% set ts = "2026-03-09T14:05:00+00:00" %}{{ ts | localdate }}"#,
        );
        assert_eq!(out, "M3 9, 2026");
    }

    #[test]
    fn localdate_datetime_style_zero_pads_time() {
        // Fluent would render a bare number as "5", never "05", so the filter
        // pads before handing the values over.
        let out = render_with(
            Arc::new(FormatTranslator),
            r#"{% set ts = "2026-03-09T04:05:00+00:00" %}{{ ts | localdate(style="datetime") }}"#,
        );
        assert_eq!(out, "M3 9, 2026 04:05");
    }

    #[test]
    fn localdate_passes_through_unparseable_input() {
        // A malformed timestamp must not take down the page it appears on.
        let out = render_with(
            Arc::new(FormatTranslator),
            r#"{{ "not-a-date" | localdate }}"#,
        );
        assert_eq!(out, "not-a-date");
    }

    #[test]
    fn thousands_separator_comes_from_the_catalog() {
        let out = render_with(Arc::new(FormatTranslator), "{{ 15000000 | thousands }}");
        assert_eq!(out, "15.000.000");
    }

    #[test]
    fn thousands_falls_back_to_comma_when_catalog_lacks_the_key() {
        // StubTranslator returns the key itself, which is far longer than one
        // character — the filter must not splice that in as a separator.
        let out = render_with(Arc::new(StubTranslator), "{{ 15000000 | thousands }}");
        assert_eq!(out, "15,000,000");
    }

    #[test]
    fn translated_output_is_html_escaped() {
        // Catalogs are admin-editable, so a translation must never be trusted as
        // markup. If this ever fails, `| safe` has crept in somewhere.
        struct HostileTranslator;
        #[async_trait::async_trait]
        impl Translator for HostileTranslator {
            fn translate(&self, _: &Locale, _: &str, _: &[(&str, TransArg)]) -> String {
                "<script>alert(1)</script>".to_string()
            }
            fn has_key(&self, _: &Locale, _: &str) -> bool {
                true
            }
            fn available_locales(&self) -> Vec<Locale> {
                vec![Locale::default_locale()]
            }
            fn default_locale_keys(&self) -> Vec<String> {
                Vec::new()
            }
            async fn reload(&self) -> std::result::Result<(), ferum_application::shared::AppError> {
                Ok(())
            }
        }

        let s = Scratch::new("t-escape").with_default_theme();
        s.write("themes/default/templates/p.html", r#"{{ t(k="x") }}"#);

        let translator: Arc<dyn Translator> = Arc::new(HostileTranslator);
        let tera = TeraEngine::build_tera(
            &s.0.join("themes"),
            &s.0.join("templates"),
            &s.0.join("static"),
            &Locale::default_locale(),
            &translator,
        )
        .unwrap();

        let out = tera
            .render("default/templates/p.html", &Context::new())
            .unwrap();
        assert!(!out.contains("<script>"), "translation was not escaped: {out}");
    }
}
