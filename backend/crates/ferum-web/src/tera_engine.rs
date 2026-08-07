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
        themes_dir: &std::path::Path,
        admin_templates_dir: &std::path::Path,
        static_dir: &std::path::Path,
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
    ///
    /// `pub` so `tests/web` can drive both policies against a synthetic tree —
    /// the fail-closed path cannot be reached through `TeraEngine::new` without
    /// a broken template checked into the repository.
    pub fn build_tera(
        themes_dir: &std::path::Path,
        admin_templates_dir: &std::path::Path,
        static_dir: &std::path::Path,
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

        // `file_url(key="…")` — the stable identity of a stored file.
        //
        // Templates used to write `/files/{{ key }}` by hand in eleven places.
        // That happened to be right, but only by coincidence: the resolver path
        // is `ports::file_url`'s business, and a template that spells it out is
        // a copy that cannot follow if it ever changes. Calling through keeps
        // one definition, exactly as `key_from_url` keeps one on the way back.
        //
        // Not `public_url`: this value ends up in `src` attributes on pages that
        // may be cached, and `public_url` names where the bytes live *today*.
        // See the identity-vs-location note on `ports::file_url`.
        tera.register_function(
            "file_url",
            move |args: &std::collections::HashMap<String, tera::Value>| {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| tera::Error::msg("file_url() requires a `key` argument"))?;
                Ok(tera::Value::String(
                    ferum_application::ports::file_url(key),
                ))
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
fn compute_asset_version(static_dir: &std::path::Path, themes_dir: &std::path::Path) -> String {
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
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
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
fn partition_theme_files(themes_dir: &std::path::Path) -> (ThemeFiles, Vec<(String, ThemeFiles)>) {
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
fn collect_html_templates(dir: &std::path::Path) -> Vec<(PathBuf, Option<String>)> {
    let mut out = Vec::new();
    if dir.exists() {
        walk_html_dir(dir, dir, &mut out);
    }
    out
}

fn walk_html_dir(base: &std::path::Path, current: &std::path::Path, out: &mut Vec<(PathBuf, Option<String>)>) {
    let Ok(entries) = std::fs::read_dir(current) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_html_dir(base, &path, out);
        } else if path.extension().is_some_and(|e| e == "html") {
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
