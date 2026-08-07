//! Guards the checked-in templates against syntax errors.
//!
//! `build_tera` fails closed on first-party templates (`frontend/templates/` and
//! the built-in `default` theme), so a broken one makes `TeraEngine::new` return
//! `Err` and these tests panic on the `expect`. The tests in [`super::tera_engine`]
//! cover that policy directly, against a synthetic tree.
//!
//! What these add is coverage of the *real* repository tree, and — because a user
//! theme fails open and is merely skipped — an assertion that each template is
//! actually present in the registry rather than quietly missing.

use std::path::PathBuf;

fn frontend_dirs() -> (PathBuf, PathBuf, PathBuf) {
    // tests/web/ -> tests/ -> backend/ -> repo root
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..");
    let frontend = root.join("frontend");
    (
        frontend.join("themes"),
        frontend.join("templates"),
        frontend.join("static"),
    )
}

fn locales_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("locales")
}

/// The real catalog, not a stub — so these tests also assert that the shipped
/// `.ftl` files parse and that `t()` is wired end to end.
async fn translator() -> std::sync::Arc<dyn ferum_application::ports::Translator> {
    std::sync::Arc::new(
        ferum_infrastructure::i18n::FluentTranslator::new(vec![locales_dir()]).await,
    )
}

async fn engine() -> ferum_web::tera_engine::TeraEngine {
    let (themes, admin, static_dir) = frontend_dirs();
    assert!(
        admin.join("mod").join("log.html").is_file(),
        "fixture path wrong: no mod/log.html under {admin:?}"
    );
    ferum_web::tera_engine::TeraEngine::new(themes, admin, static_dir, translator().await)
        .expect("TeraEngine::new must succeed")
}

/// Admin/mod templates must parse. Listed explicitly rather than globbed, so a
/// template silently vanishing from disk also fails the test.
#[tokio::test]
async fn admin_and_mod_templates_are_loaded() {
    let tera = engine().await;

    for name in [
        "mod/log.html",
        "mod/queue.html",
        "mod/reports.html",
        "mod/threads.html",
        "mod/users.html",
        "admin/base.html",
        "admin/dashboard.html",
        "admin/settings.html",
    ] {
        assert_eq!(
            tera.first_existing_template(&[name.to_string()]).await,
            Some(name.to_string()),
            "{name} failed to parse or was not loaded (see build_tera ERROR logs)"
        );
    }
}

/// The default theme chain must resolve too.
#[tokio::test]
async fn default_theme_templates_are_loaded() {
    let tera = engine().await;

    for name in [
        "default/templates/base.html",
        "default/templates/home.html",
        "default/templates/forum/thread.html",
    ] {
        assert_eq!(
            tera.first_existing_template(&[name.to_string()]).await,
            Some(name.to_string()),
            "{name} failed to parse or was not loaded"
        );
    }
}
