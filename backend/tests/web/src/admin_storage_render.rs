//! Renders `/admin/storage` against the shipped template and catalog.
//!
//! `tera_templates.rs` proves the template parses. That would still pass if
//! every `adm-*` key were missing, because Fluent renders an unknown key as the
//! key itself — the page would read "adm-sweep-title" and nothing would error.
//!
//! The other thing worth pinning is that both delete buttons ship **disabled**.
//! They are enabled by `ferum-admin-storage.js` only after a scan finishes, and
//! that ordering is the entire reason this page exists rather than a curl
//! command: the list gets read before anything acts on it.

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
    // Mirrors what `handlers::admin::render_admin` injects on every admin page.
    // Built by hand here because that function needs an `AppState`, which this
    // suite cannot construct.
    ctx.insert("default_theme_slug", "default");
    ctx.insert("locale", "en");
    ctx.insert("available_locales", &json!(["en", "vi"]));
    ctx.insert("current_path", "/admin/storage");
    ctx.insert("js_strings", &json!({}));

    engine
        .render(&Locale::default_locale(), "admin/storage.html", ctx)
        .await
        .unwrap_or_else(|e| panic!("admin/storage.html must render: {e:?}"))
}

#[tokio::test]
async fn both_tools_are_present_and_explained() {
    let html = render().await;

    for id in [
        "sweep-scan",
        "sweep-apply",
        "sweep-results",
        "audit-scan",
        "audit-apply",
        "audit-results",
    ] {
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "ferum-admin-storage.js drives #{id}; the template must provide it"
        );
    }

    assert!(html.contains("/static/js/ferum-admin-storage.js"));
}

#[tokio::test]
async fn every_admin_key_resolves_to_real_text() {
    let html = render().await;

    // Fluent echoes an unknown key back, so a raw `adm-` token in the output is a
    // missing entry in locales/en/admin.ftl.
    for key in [
        "adm-storage",
        "adm-scan",
        "adm-storage-intro",
        "adm-sweep-title",
        "adm-sweep-help",
        "adm-audit-title",
        "adm-audit-help",
        "adm-delete-found",
        "adm-release-found",
    ] {
        assert!(
            !html.contains(key),
            "the page rendered the raw key `{key}` — add it to locales/en/admin.ftl"
        );
    }
}

#[tokio::test]
async fn the_delete_buttons_start_disabled() {
    let html = render().await;

    // Both destructive buttons must arrive disabled. Shipping either enabled
    // would make deletion reachable before anything has been scanned or read,
    // which is the failure this whole page is meant to prevent.
    let enabled_apply = html
        .split("id=\"sweep-apply\"")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .map(|attrs| attrs.contains("disabled"));
    assert_eq!(
        enabled_apply,
        Some(true),
        "#sweep-apply must render with `disabled`"
    );

    let audit_apply = html
        .split("id=\"audit-apply\"")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .map(|attrs| attrs.contains("disabled"));
    assert_eq!(
        audit_apply,
        Some(true),
        "#audit-apply must render with `disabled`"
    );
}
