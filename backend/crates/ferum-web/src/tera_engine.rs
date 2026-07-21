use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tera::{Context, Tera};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct TeraEngine {
    tera: Arc<RwLock<Arc<Tera>>>,
    themes_dir: PathBuf,
    admin_templates_dir: PathBuf,
    static_dir: PathBuf,
}

impl TeraEngine {
    pub fn new(
        themes_dir: PathBuf,
        admin_templates_dir: PathBuf,
        static_dir: PathBuf,
    ) -> Result<Self> {
        let tera = Self::build_tera(&themes_dir, &admin_templates_dir, &static_dir)?;
        Ok(Self {
            tera: Arc::new(RwLock::new(Arc::new(tera))),
            themes_dir,
            admin_templates_dir,
            static_dir,
        })
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
        // assets (e.g. the widgets bundle). The token is derived once — at load /
        // reload time — from the mtime of the compiled widgets bundle. When the
        // bundle is rebuilt its mtime changes, so the URL `?v=<token>` changes and
        // browsers fetch the fresh file instead of a stale cached copy. Without this,
        // `Cache-Control: max-age=86400` on /static means client-side fixes can take
        // up to a day to reach returning users.
        let token = compute_asset_version(static_dir);
        tera.register_function(
            "asset_version",
            move |_args: &std::collections::HashMap<String, tera::Value>| {
                Ok(tera::Value::String(token.clone()))
            },
        );

        // `thousands` filter — groups an integer with comma separators:
        // 15000000 → "15,000,000". Non-numeric input passes through
        // unchanged so a template never errors on a missing price.
        tera.register_filter(
            "thousands",
            |value: &tera::Value, _args: &std::collections::HashMap<String, tera::Value>| {
                match value.as_i64().or_else(|| value.as_f64().map(|f| f as i64)) {
                    Some(n) => Ok(tera::Value::String(group_thousands(n))),
                    None => Ok(value.clone()),
                }
            },
        );

        Ok(tera)
    }

    /// Reload theme templates from disk without restarting. Call after theme upload.
    pub async fn reload_themes(&self) -> Result<()> {
        let themes_dir = self.themes_dir.clone();
        let admin_dir = self.admin_templates_dir.clone();
        let static_dir = self.static_dir.clone();
        // build_tera does blocking file I/O + CPU-bound compilation — must not run on
        // the async worker thread or it will stall all concurrent HTTP requests.
        let fresh = tokio::task::spawn_blocking(move || {
            Self::build_tera(&themes_dir, &admin_dir, &static_dir)
        })
            .await
            .map_err(|e| anyhow::anyhow!("build_tera join error: {e}"))??;
        *self.tera.write().await = Arc::new(fresh);
        tracing::info!("Tera templates reloaded from disk");
        Ok(())
    }

    /// Return the first candidate template name that exists, holding only one lock.
    /// Used by render_with_theme() to walk the inheritance chain without multiple async round-trips.
    pub async fn first_existing_template(&self, candidates: &[String]) -> Option<String> {
        let tera = self.tera.read().await;
        candidates.iter().find(|n| tera.get_template(n).is_ok()).cloned()
    }

    /// Render a template by name with the given context.
    pub async fn render(&self, template_name: &str, ctx: &Context) -> Result<String> {
        // Clone the Arc<Tera> (one atomic increment) while holding the read lock briefly,
        // then release the lock before the CPU-bound render. This avoids copying all
        // compiled template ASTs on every request, which was the previous bottleneck.
        let lock_start = std::time::Instant::now();
        let tera = Arc::clone(&*self.tera.read().await);
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

/// Derive a cache-busting token from the newest mtime among the app-owned JS
/// assets. Returns seconds-since-epoch (stringified); falls back to "dev" when
/// none can be read so templates still render a valid URL. Editing or rebuilding
/// any of these files bumps the token, so `?v=<token>` changes and browsers fetch
/// the fresh file instead of a stale copy held by `Cache-Control: max-age=86400`.
fn compute_asset_version(static_dir: &PathBuf) -> String {
    const APP_ASSETS: [&str; 7] = [
        "ferum-widgets.iife.js",
        "ferum-api.js",
        "ferum-utils.js",
        "ferum-admin.js",
        "ferum-admin-themes.js",
        "ferum-admin-plugins.js",
        "ferum-admin-plugins-detail.js",
    ];
    let js_dir = static_dir.join("js");
    APP_ASSETS
        .iter()
        .filter_map(|name| {
            std::fs::metadata(js_dir.join(name))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
        })
        .max()
        .map(|secs| secs.to_string())
        .unwrap_or_else(|| "dev".to_string())
}

/// Group an integer with comma thousands-separators: 15000000 → "15,000,000".
/// Negatives keep their sign.
fn group_thousands(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    let bytes = digits.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
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
            TeraEngine::build_tera(
                &self.0.join("themes"),
                &self.0.join("templates"),
                &self.0.join("static"),
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
        TeraEngine::build_tera(
            &root.join("themes"),
            &root.join("templates"),
            &root.join("static"),
        )
        .expect("the checked-in templates must parse");
    }
}
