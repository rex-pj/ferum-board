//! Renders the admin plugin-detail page for real, against the shipped template
//! and catalogs.
//!
//! `tera_templates.rs` proves the template *parses*. That is not the same thing,
//! and the config field reference this file covers is exactly the gap: it is a
//! `{% for key, prop in plugin.config_schema.properties %}` over a `serde_json`
//! value the handler forwards straight from the plugin's manifest. Every way of
//! getting that wrong — iterating a value that is not a map, `in` against an
//! absent `required` list, a member lookup on a property that omits the field —
//! fails at render, not at parse.
//!
//! The field reference is also the ONLY surface on which a plugin's own
//! documentation of its settings reaches the operator. The config editor is a
//! raw JSON textarea, so a constraint the server cannot enforce — `home-hero`'s
//! image URLs must be same-origin or the Content-Security-Policy blanks the tile
//! with nothing in the server log — is communicated here or nowhere.

use std::path::PathBuf;

use ferum_domain::Locale;
use ferum_web::view_models::page_context::{CurrentUserCtx, SiteCtx};
use serde_json::json;
use tera::Context;

const TEMPLATE: &str = "admin/plugins/detail.html";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

async fn engine() -> ferum_web::tera_engine::TeraEngine {
    let frontend = repo_root().join("frontend");
    let translator = std::sync::Arc::new(
        ferum_infrastructure::i18n::FluentTranslator::new(vec![repo_root().join("locales")]).await,
    );
    ferum_web::tera_engine::TeraEngine::new(
        frontend.join("themes"),
        frontend.join("templates"),
        frontend.join("static"),
        translator,
    )
    .expect("TeraEngine::new must succeed")
}

/// Mirrors what `handlers::admin::pages::plugins::detail` inserts. `config_schema`
/// is passed through from the manifest untouched by the handler, so a test that
/// invented a tidier shape here would not be testing the real input.
fn ctx_with_schema(config_schema: serde_json::Value) -> Context {
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
    // The real struct, not a hand-rolled JSON object: the admin shell renders
    // partials/notif_bell.html and the user dropdown from it, so a field added
    // to CurrentUserCtx should break this file rather than have it drift.
    ctx.insert(
        "current_user",
        &CurrentUserCtx {
            id: "00000000-0000-0000-0000-0000000000a1".into(),
            username: "admin".into(),
            display_name: "Admin".into(),
            avatar_url: None,
            is_admin: true,
            is_moderator: true,
            unread_count: 0,
            theme: "auto".into(),
            font_size: "medium".into(),
            layout: "comfortable".into(),
            // None = follow the device's zone, which is what a user who never
            // opened the setting has.
            timezone: None,
        },
    );
    ctx.insert("flash_success", &json!(null));
    ctx.insert("flash_error", &json!(null));
    ctx.insert("js_strings", &json!({}));
    // The admin shell, as injected by `handlers::admin::render_admin`. Kept in
    // the same order as that function so a variable added there is easy to
    // mirror here — the failure mode otherwise is every test in this file
    // failing on a missing variable that has nothing to do with what they check.
    ctx.insert("default_theme_slug", "default");
    ctx.insert("locale", "en");
    ctx.insert("current_path", "/admin/plugins/com.ferum.home-hero");
    ctx.insert("available_locales", &json!(["en", "vi"]));
    ctx.insert(
        "plugin",
        &json!({
            "slug": "com.ferum.home-hero",
            "name": "Home Hero",
            "version": "1.0.0",
            "tier": "script",
            "status": "active",
            "is_active": true,
            "error_message": null,
            "circuit_open": false,
            "config_json": "{}",
            "config_schema": config_schema,
            "install_path": "plugins/com.ferum.home-hero",
            "installed_at": "2026-08-06 10:00 UTC",
            "activated_at": "2026-08-06 10:01 UTC",
            "logs": [],
        }),
    );
    ctx
}

async fn render(config_schema: serde_json::Value) -> String {
    engine()
        .await
        .render(&Locale::default_locale(), TEMPLATE, ctx_with_schema(config_schema))
        .await
        .expect("admin/plugins/detail.html must render")
}

/// The real `home-hero` schema, shape for shape. If the manifest gains a
/// construct this template cannot walk, this is where it surfaces.
fn home_hero_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "required": [],
        "properties": {
            "default_locale": {
                "type": "string",
                "title": "Fallback locale",
                "description": "Used when the page's language has no entry under `locales`.",
                "default": "en"
            },
            "locales": {
                "type": "object",
                "title": "Masthead text, per language",
                "description": "Keyed by language code and matched against the page's language."
            },
            "tiles": {
                "type": "array",
                "title": "Photo mosaic (up to 4)",
                "description": "image_url must be an address on this site (/files/…) or on your configured CDN. \
                                An image hosted anywhere else is blocked by the browser's Content-Security-Policy."
            }
        }
    })
}

#[tokio::test]
async fn field_reference_lists_every_declared_property() {
    let html = render(home_hero_schema()).await;

    for key in ["default_locale", "locales", "tiles"] {
        assert!(
            html.contains(key),
            "config_schema property `{key}` is missing from the rendered field reference"
        );
    }
    assert!(
        html.contains("Masthead text, per language"),
        "property titles must reach the page — they are the plain-language name of each field"
    );
}

/// The reason this whole surface was built. A plugin can declare a rule the
/// server has no way to check, and the raw JSON textarea says nothing about it;
/// if the description does not render, the operator's only warning is a README
/// they never open.
#[tokio::test]
async fn unenforceable_constraints_reach_the_operator() {
    let html = render(home_hero_schema()).await;

    assert!(
        html.contains("Content-Security-Policy"),
        "the CSP constraint on tile image URLs must be visible beside the config editor"
    );
    assert!(
        html.contains("configured CDN"),
        "the description must survive rendering intact, not be truncated to its first clause"
    );
}

#[tokio::test]
async fn type_and_default_are_shown_when_declared() {
    let html = render(home_hero_schema()).await;

    // `type` and `default` are both member lookups on a serde_json map, and
    // `default` is also a Tera filter name — a lookup that resolved to the filter
    // instead of the field would render nothing here.
    assert!(html.contains("string"), "declared types must be shown");
    assert!(
        html.contains("Default"),
        "a property carrying `default` must render the default label"
    );
}

/// `required` is optional in a manifest, and `in` against an undefined value is a
/// render-time error rather than a false — which is why the template nests two
/// ifs instead of writing `required and key in required`. Both shapes are
/// exercised because only one of them existed when the template was written.
#[tokio::test]
async fn required_list_may_be_present_absent_or_populated() {
    let absent = render(json!({
        "type": "object",
        "properties": { "message": { "type": "string", "title": "Banner message" } }
    }))
    .await;
    assert!(absent.contains("Banner message"));
    assert!(
        !absent.contains("Required"),
        "no property can be marked required when the manifest declares no required list"
    );

    let populated = render(json!({
        "type": "object",
        "required": ["message"],
        "properties": {
            "message": { "type": "string", "title": "Banner message" },
            "kind": { "type": "string", "title": "Alert style" }
        }
    }))
    .await;
    assert!(
        populated.contains("Required"),
        "a property named in `required` must be badged"
    );
}

/// Most plugins declare no config at all. The block is skipped rather than
/// rendering an empty heading over nothing.
#[tokio::test]
async fn plugin_without_a_schema_renders_without_the_section() {
    let html = render(json!({})).await;

    assert!(
        html.contains("id=\"plugin-config-editor\""),
        "the config editor itself must still render for a plugin with no schema"
    );
    assert!(
        !html.contains("adm-config-fields"),
        "a missing translation would render the raw key — the section must be skipped, not broken"
    );
}
