//! Renders the example themes' own pages for real, to pin the two halves of
//! theme inheritance that nothing else checks.
//!
//! `tera_templates.rs` proves a template *parses*, which is not the same thing:
//! ferum-arcade and ferum-sumi used to ship full clones of the default
//! `base.html`, and those clones parsed perfectly while having silently lost the
//! `ferum-i18n` meta (so every `Ferum.t()` string rendered as its raw key), the
//! Open Graph and Twitter Card tags, and the hreflang alternates. Nothing failed
//! — the pages just quietly served less than they should.
//!
//! So this asserts both directions at once: that a theme extending the default
//! still RECEIVES the shell it inherits, and that its own chrome — emitted
//! through block overrides with `{{ super() }}` — survives the inheritance.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ferum_domain::Locale;
use ferum_web::view_models::page_context::SiteCtx;
use serde_json::json;
use tera::Context;

/// The example themes this file renders.
const EXAMPLE_THEMES: [&str; 2] = ["ferum-arcade", "ferum-sumi"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// A themes root staged from the two COMMITTED sources: `frontend/themes/default`
/// plus each example theme's own directory under `examples/themes/`.
///
/// Rendering straight out of `frontend/themes` is what this test used to do, and
/// it is not reproducible: `/frontend/themes/*/` is gitignored except `default/`,
/// so ferum-arcade and ferum-sumi are only there on a machine where an admin has
/// installed the archives. A fresh checkout has neither — which is why this
/// passed on developer machines and failed in CI with "Template not found".
/// Worse, where they *are* installed they can be a different version from the
/// source (they were: 1.0.0 on disk against 1.1.0 in `examples/`), so the test
/// was asserting against whatever each developer happened to have extracted.
///
/// `examples/themes/<slug>/` is also what the `.zip` beside it is packaged from,
/// so this checks the artifact that actually ships. Only `templates/` is copied:
/// `.html` files are the only thing `TeraEngine` reads out of a themes dir, and
/// skipping the rest keeps `default/node_modules/` out of the walk.
fn staged_themes_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = repo_root();
        let dir =
            std::env::temp_dir().join(format!("ferum-theme-inheritance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        copy_html_tree(&root.join("frontend/themes/default/templates"), &dir.join("default/templates"));
        for slug in EXAMPLE_THEMES {
            copy_html_tree(
                &root.join("examples/themes").join(slug).join("templates"),
                &dir.join(slug).join("templates"),
            );
        }
        dir
    })
}

/// Copy every `.html` file under `src` into `dst`, preserving the sub-tree.
fn copy_html_tree(src: &Path, dst: &Path) {
    let entries = std::fs::read_dir(src)
        .unwrap_or_else(|e| panic!("theme source {} must exist: {e}", src.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_html_tree(&path, &dst.join(entry.file_name()));
        } else if path.extension().is_some_and(|ext| ext == "html") {
            std::fs::create_dir_all(dst).expect("staging dir must be creatable");
            std::fs::copy(&path, dst.join(entry.file_name())).expect("template must be copyable");
        }
    }
}

async fn engine() -> ferum_web::tera_engine::TeraEngine {
    let frontend = repo_root().join("frontend");
    let translator = std::sync::Arc::new(
        ferum_infrastructure::i18n::FluentTranslator::new(vec![repo_root().join("locales")]).await,
    );
    ferum_web::tera_engine::TeraEngine::new(
        staged_themes_dir().to_path_buf(),
        frontend.join("templates"),
        frontend.join("static"),
        translator,
    )
    .expect("TeraEngine::new must succeed")
}

fn ctx(active_theme: &str) -> Context {
    let mut c = Context::new();
    c.insert(
        "site",
        &SiteCtx {
            name: "Ferum".into(),
            slogan: "Reviews".into(),
            tagline: "Furniture reviews".into(),
            logo_url: None,
            favicon_url: None,
            primary_color: None,
            primary_color_rgb: None,
            url: "http://localhost:5173".into(),
        },
    );
    c.insert("current_user", &json!(null));
    c.insert("active_theme", active_theme);
    c.insert("nav_categories", &json!([]));
    c.insert("threads", &json!([]));
    c.insert("tags", &json!([]));
    c.insert("active_tag", &json!(null));
    c.insert("plugin_slots", &json!({}));
    c.insert("plugin_assets", &json!([]));
    c.insert("theme_bs_theme", "dark");
    c.insert("locale", "en");
    c.insert("current_path", "/");
    c.insert("js_strings", &json!({ "js-loading": "Loading" }));
    c.insert("available_locales", &json!(["en", "vi"]));
    c
}

async fn render(theme: &str) -> String {
    engine()
        .await
        .render(
            &Locale::default_locale(),
            &format!("{theme}/templates/home.html"),
            ctx(theme),
        )
        .await
        .unwrap_or_else(|e| panic!("{theme} home.html must render: {e:?}"))
}

#[tokio::test]
async fn arcade_inherits_the_default_shell_and_keeps_its_own_chrome() {
    let html = render("ferum-arcade").await;
    // Recovered by inheriting (each was absent from the cloned base):
    assert!(html.contains("ferum-i18n"), "js i18n meta missing");
    assert!(html.contains("og:site_name"), "Open Graph missing");
    assert!(html.contains("twitter:card"), "Twitter Card missing");
    assert!(html.contains("hreflang"), "hreflang alternates missing");
    // Arcade's own chrome, via block overrides:
    assert!(html.contains("fac-scanlines"), "scanline overlay lost");
    assert!(html.contains("fac-hud-frame"), "HUD frame lost");
    // `{{ super() }}` really pulled the parent nav in:
    assert!(html.contains("fr-bottom-nav"), "bottom nav lost");
}

#[tokio::test]
async fn sumi_inherits_the_default_shell_and_keeps_its_own_chrome() {
    let html = render("ferum-sumi").await;
    assert!(html.contains("ferum-i18n"), "js i18n meta missing");
    assert!(html.contains("og:site_name"), "Open Graph missing");
    assert!(html.contains("hreflang"), "hreflang alternates missing");
    assert!(html.contains("fsm-lintel"), "torii lintel lost");
    assert!(html.contains("fsm-hanko"), "hanko seal lost");
    assert!(html.contains("fr-bottom-nav"), "bottom nav lost (super() broken)");
}

/// The lintel sits in normal flow, so it must precede the nav.
#[tokio::test]
async fn sumi_lintel_precedes_the_nav() {
    let html = render("ferum-sumi").await;
    let lintel = html.find("fsm-lintel").expect("lintel present");
    let nav = html.find("fr-navbar").or_else(|| html.find("<nav")).expect("nav present");
    assert!(lintel < nav, "lintel must render above the nav");
}
