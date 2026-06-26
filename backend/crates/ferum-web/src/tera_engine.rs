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

    fn build_tera(
        themes_dir: &PathBuf,
        admin_templates_dir: &PathBuf,
        static_dir: &PathBuf,
    ) -> Result<Tera> {
        // Load theme templates as raw strings and pass them as a single batch.
        // Tera::new() with a glob produces OS-separator template names (backslashes on Windows)
        // which don't match the forward-slash names used by handlers. Reading content ourselves
        // also lets us surface per-file I/O errors without aborting the whole load.
        let theme_files = collect_html_templates(themes_dir);
        let mut raw_templates: Vec<(String, String)> = Vec::with_capacity(theme_files.len());
        for (path, name_opt) in &theme_files {
            let Some(name) = name_opt else { continue };
            match std::fs::read_to_string(path) {
                Ok(content) => raw_templates.push((name.clone(), content)),
                Err(e) => tracing::warn!("Failed to read theme template {:?}: {}", path, e),
            }
        }

        let mut tera = Tera::default();
        if raw_templates.is_empty() {
            tracing::warn!("No theme templates found at: {}", themes_dir.display());
        } else {
            let pairs: Vec<(&str, &str)> = raw_templates
                .iter()
                .map(|(n, c)| (n.as_str(), c.as_str()))
                .collect();
            match tera.add_raw_templates(pairs) {
                Ok(()) => {
                    tracing::info!("Theme templates loaded: {} files", raw_templates.len());
                }
                Err(e) => {
                    tracing::error!("Tera theme templates failed to load: {}", e);
                    // Templates parsed before the error are still in tera.templates;
                    // log registered names so we can spot what is missing.
                    let registered: Vec<&str> = raw_templates
                        .iter()
                        .filter(|(n, _)| tera.get_template(n).is_ok())
                        .map(|(n, _)| n.as_str())
                        .collect();
                    tracing::error!("Templates registered despite error: {:?}", registered);
                }
            }
        }

        // Load admin templates with explicit forward-slash names.
        // We walk the directory manually instead of using Tera::new() + extend() to guarantee
        // correct template names on all platforms — on Windows, glob paths use backslashes which
        // would produce names like "admin\dashboard.html" instead of "admin/dashboard.html".
        let admin_files = collect_html_templates(admin_templates_dir);
        if admin_files.is_empty() {
            tracing::warn!("No admin templates found at: {}", admin_templates_dir.display());
        } else {
            let mut loaded = 0usize;
            let mut failed = 0usize;
            for (path, name_opt) in &admin_files {
                let Some(name) = name_opt else { continue };
                if let Err(e) = tera.add_template_files(vec![(path.clone(), Some(name.clone()))]) {
                    let mut chain = format!("{}", e);
                    let mut src: &dyn std::error::Error = &e;
                    while let Some(cause) = src.source() {
                        chain.push_str(&format!(" -> {}", cause));
                        src = cause;
                    }
                    tracing::error!("Failed to load admin template '{}': {}", name, chain);
                    failed += 1;
                } else {
                    loaded += 1;
                }
            }
            if failed > 0 {
                tracing::warn!("Admin templates: {} loaded, {} FAILED (see ERROR lines above)", loaded, failed);
            } else {
                tracing::info!("Admin templates loaded: {} files", loaded);
            }
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

    #[test]
    fn test_theme_templates_load() {
        let themes_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../themes");
        let files = collect_html_templates(&themes_dir);
        assert!(!files.is_empty(), "No templates found — check path ../../../themes");

        let mut raw: Vec<(String, String)> = Vec::new();
        for (path, name_opt) in &files {
            let Some(name) = name_opt else { continue };
            let content = std::fs::read_to_string(path).expect("read file");
            raw.push((name.clone(), content));
        }
        let pairs: Vec<(&str, &str)> = raw.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();
        let mut tera = tera::Tera::default();
        if let Err(e) = tera.add_raw_templates(pairs) {
            panic!("Theme template batch load failed: {}", e);
        }
    }
}
