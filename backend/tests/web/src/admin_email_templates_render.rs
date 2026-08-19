//! Renders `/admin/email-templates` against the shipped template and catalog.
//!
//! `tera_templates.rs` proves the template parses, which would still pass with
//! every `adm-*` key missing — Fluent echoes an unknown key back, so the page
//! would read "adm-subject" and nothing would error.
//!
//! The property worth pinning hardest is the preview iframe's `sandbox`. It
//! renders admin-authored HTML, and `srcdoc` inherits the embedding origin
//! unless the sandbox withholds it; an `allow-same-origin` added later would
//! turn the editor into a self-XSS surface with no visible symptom.

use std::path::PathBuf;

use ferum_domain::Locale;
use ferum_web::view_models::page_context::SiteCtx;
use serde_json::json;
use tera::Context;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

async fn render() -> String {
    let frontend = repo_root().join("frontend");
    let translator = std::sync::Arc::new(
        ferum_infrastructure::i18n::FluentTranslator::new(vec![repo_root().join("locales")]).await,
    );
    let engine = ferum_web::tera_engine::TeraEngine::new(
        frontend.join("themes"),
        frontend.join("templates"),
        frontend.join("static"),
        translator,
    )
    .expect("TeraEngine::new must succeed");

    let mut ctx = Context::new();
    ctx.insert(
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
    ctx.insert(
        "current_user",
        &json!({
            "id": "00000000-0000-0000-0000-00000000a001",
            "username": "root",
            "display_name": "Root",
            "avatar_url": null,
            "trust_level": "leader",
            "is_admin": true,
            "is_mod": true,
            "unread_count": 0
        }),
    );
    ctx.insert("default_theme_slug", "default");
    ctx.insert("locale", "en");
    ctx.insert("available_locales", &json!(["en", "vi"]));
    ctx.insert("current_path", "/admin/email-templates");
    ctx.insert("js_strings", &json!({}));
    ctx.insert("template_locales", &json!(["en", "vi"]));

    engine
        .render(&Locale::default_locale(), "admin/email_templates.html", ctx)
        .await
        .unwrap_or_else(|e| panic!("admin/email_templates.html must render: {e:?}"))
}

#[tokio::test]
async fn every_element_the_script_drives_is_present() {
    let html = render().await;

    for id in [
        "tpl-list",
        "tpl-list-shared",
        "tpl-shared-label",
        "tpl-locale",
        "tpl-editor",
        "tpl-name",
        "tpl-key",
        "tpl-description",
        "tpl-variables",
        "tpl-subject",
        "tpl-subject-inert",
        "tpl-body",
        "tpl-status",
        "tpl-save-btn",
        "tpl-preview-btn",
        "tpl-test-btn",
        "tpl-reset-btn",
        "tpl-preview-card",
        "tpl-preview-frame",
        "tpl-preview-subject",
        "tpl-preview-text",
        "tpl-view-html",
        "tpl-view-text",
        "tpl-customised",
        "tpl-default",
        "tpl-dirty",
    ] {
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "ferum-admin-email-templates.js drives #{id}; the template must provide it"
        );
    }

    assert!(html.contains("/static/js/ferum-admin-email-templates.js"));
}

/// The security property, stated as a test so removing it is a failing build
/// rather than a quiet regression.
#[tokio::test]
async fn the_preview_iframe_is_sandboxed_without_same_origin() {
    let html = render().await;

    let frame = html
        .split("id=\"tpl-preview-frame\"")
        .nth(1)
        .expect("the preview iframe must exist");
    let tag = &frame[..frame.find('>').expect("the iframe tag must close")];

    assert!(
        tag.contains("sandbox"),
        "admin-authored HTML must render sandboxed: {tag}"
    );
    assert!(
        !tag.contains("allow-same-origin"),
        "allow-same-origin gives the previewed document this origin's privileges: {tag}"
    );
}

/// The list holds only the copy strings the script needs; putting them in the JS
/// would put English in a file the catalog cannot reach.
#[tokio::test]
async fn the_scripts_prompt_strings_come_from_the_catalog() {
    let html = render().await;

    // The shared dialog takes a title, a body and a button label separately, so
    // all three have to reach the script.
    for attr in [
        "data-edited-text",
        "data-discard-title",
        "data-discard-text",
        "data-discard-ok",
    ] {
        assert!(html.contains(attr), "#tpl-list must carry {attr}");
    }
    for attr in [
        "data-saved-text",
        "data-confirm-title",
        "data-confirm-text",
        "data-confirm-ok",
    ] {
        assert!(html.contains(attr), "the action buttons must carry {attr}");
    }
}

#[tokio::test]
async fn the_locale_picker_lists_the_installed_locales() {
    let html = render().await;
    assert!(html.contains("<option value=\"en\">"), "en must be offered");
    assert!(html.contains("<option value=\"vi\">"), "vi must be offered");
}

/// A plain substring check, so a key must not be a prefix of anything else in
/// the page. `adm-body` was, colliding with the `adm-body` layout class in
/// `admin/base.html` and failing this for the wrong reason — hence
/// `adm-body-html`.
#[tokio::test]
async fn every_admin_key_resolves_to_real_text() {
    let html = render().await;

    for key in [
        "adm-email-templates-intro",
        "adm-language",
        "adm-shared",
        "adm-customised",
        "adm-using-default",
        "adm-unsaved",
        "adm-unsaved-changes",
        "adm-discard",
        "adm-layout-has-no-subject",
        "adm-insert-variable",
        "adm-variables-help",
        "adm-discard-changes",
        "adm-subject",
        "adm-body-html",
        "adm-preview-help",
        "adm-plain-text-part",
        "adm-send-test",
        "adm-reset-to-default",
        "adm-reset-confirm",
        "adm-template-saved",
        "adm-template-reset",
    ] {
        assert!(
            !html.contains(key),
            "{key} rendered as its own name — the entry is missing from locales/en/admin.ftl"
        );
    }
}
