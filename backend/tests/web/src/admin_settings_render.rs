//! Renders `admin/settings.html` for real, across the three mail-provider states.
//!
//! `tera_templates.rs` proves the template parses, and `settings_template_contract.rs`
//! scans its source. Neither catches the failure this file exists for: Tera resolves
//! a *variable* at render time, so a context key the handler forgot to insert — or
//! renamed — is a 500 on the settings page and nothing before that point notices.
//!
//! Three keys were added to this page's context (`smtp_pass_set`, `mail_provider`,
//! `secrets_encrypted`) and each drives a branch, so every combination has to
//! render.

use std::path::PathBuf;

use ferum_domain::Locale;
use ferum_web::handlers::admin::api::config::{CONFIG_SECRET_KEYS, CONFIG_WRITABLE_KEYS};
use ferum_web::view_models::page_context::{CurrentUserCtx, SiteCtx};
use serde_json::json;
use tera::Context;

const TEMPLATE: &str = "admin/settings.html";

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

/// Mirrors `handlers::admin::pages::settings`, in the same order, so a variable
/// added there is easy to mirror here.
fn ctx(mail_provider: &str, smtp_pass_set: bool, secrets_encrypted: bool) -> Context {
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
            timezone: None,
        },
    );
    ctx.insert("flash_success", &json!(null));
    ctx.insert("flash_error", &json!(null));
    ctx.insert("js_strings", &json!({}));
    ctx.insert("default_theme_slug", "default");
    ctx.insert("locale", "en");
    ctx.insert("current_path", "/admin/settings");
    ctx.insert("available_locales", &json!(["en", "vi"]));

    // The stored settings, minus secrets — exactly the shape `split_secrets`
    // returns.
    //
    // Built from `CONFIG_WRITABLE_KEYS` rather than a hand-listed object, for two
    // reasons. It cannot drift when a setting is added. And it encodes something
    // this test discovered: **Tera fails the render on a missing variable**, so the
    // page needs every key it reads to be present in `site_config`. In production
    // `PgSystemSeedService` guarantees that; a key added to the template without a
    // seeded default would 500 this page, and that is worth having a test notice.
    let mut config = serde_json::Map::new();
    for key in CONFIG_WRITABLE_KEYS {
        if CONFIG_SECRET_KEYS.contains(key) {
            continue; // stripped by `split_secrets` before the template sees it
        }
        config.insert((*key).to_string(), json!(""));
    }
    // The few values the assertions below actually read.
    config.insert("site_name".into(), json!("Ferum"));
    config.insert("smtp_host".into(), json!("smtp.example.com"));
    config.insert("smtp_port".into(), json!("587"));
    config.insert("smtp_user".into(), json!("apikey"));
    ctx.insert("config", &serde_json::Value::Object(config));
    ctx.insert("smtp_pass_set", &smtp_pass_set);
    ctx.insert("mail_provider", mail_provider);
    ctx.insert("secrets_encrypted", &secrets_encrypted);
    ctx
}

async fn render(mail_provider: &str, smtp_pass_set: bool, secrets_encrypted: bool) -> String {
    engine()
        .await
        .render(
            &Locale::default_locale(),
            TEMPLATE,
            ctx(mail_provider, smtp_pass_set, secrets_encrypted),
        )
        .await
        .unwrap_or_else(|e| {
            panic!("rendering {TEMPLATE} for provider {mail_provider:?} failed: {e:?}")
        })
}

#[tokio::test]
async fn renders_for_every_mail_provider_state() {
    for provider in ["resend", "smtp", "disabled"] {
        for &pass_set in &[true, false] {
            for &encrypted in &[true, false] {
                let html = render(provider, pass_set, encrypted).await;
                assert!(html.contains("SMTP"), "provider {provider}");
            }
        }
    }
}

#[tokio::test]
async fn the_smtp_password_placeholder_reflects_the_presence_flag() {
    // The affordance the leak fix had to preserve: the page must still be able to
    // say whether a password exists, without holding it.
    let set = render("smtp", true, false).await;
    assert!(set.contains("(set — enter new value to change)"));
    assert!(!set.contains("(not set)"));

    let unset = render("smtp", false, false).await;
    assert!(unset.contains("(not set)"));
    assert!(!unset.contains("(set — enter new value to change)"));
}

/// Returns the raw `<input …>` tag carrying `id="cfg-{field}"`.
///
/// Scoped to the tag rather than the tab, because the surrounding help text
/// legitimately contains the word "disable" ("Leave blank to disable email
/// sending") — a substring search over the tab passes and fails for the wrong
/// reasons.
fn input_tag<'a>(html: &'a str, field: &str) -> &'a str {
    let needle = format!("id=\"cfg-{field}\"");
    let at = html
        .find(&needle)
        .unwrap_or_else(|| panic!("no field cfg-{field} rendered"));
    let open = html[..at].rfind('<').expect("input tag start");
    let close = at + html[at..].find('>').expect("input tag end");
    &html[open..=close]
}

const SMTP_FIELDS: [&str; 4] = ["smtp_host", "smtp_port", "smtp_user", "smtp_pass"];

#[tokio::test]
async fn the_smtp_block_is_disabled_only_when_another_provider_wins() {
    // Visible but inert under Resend: an operator still needs to read what is
    // stored, because unsetting RESEND_API_KEY falls back to exactly these values.
    // `saveSettings` skips disabled inputs, which is what stops a save from
    // needlessly rebuilding a transport nothing uses.
    let resend = render("resend", true, false).await;
    for field in SMTP_FIELDS {
        assert!(
            input_tag(&resend, field).contains("disabled"),
            "cfg-{field} should be disabled while Resend is the active provider"
        );
    }

    // Editable in the other two states — including `disabled`, where the SMTP form
    // is the only way to fix the situation.
    for provider in ["smtp", "disabled"] {
        let html = render(provider, true, false).await;
        for field in SMTP_FIELDS {
            assert!(
                !input_tag(&html, field).contains("disabled"),
                "cfg-{field} must stay editable when the provider is {provider}"
            );
        }
    }
}

#[tokio::test]
async fn the_disabled_state_warns_that_addresses_are_not_being_verified() {
    // The highest-value addition of the whole change: nothing in the admin UI
    // previously said that auto-verify was on, so a public forum could be
    // accepting unverified signups with no indication anywhere.
    let html = render("disabled", false, false).await;
    assert!(html.contains("alert-warning"));
    assert!(
        html.contains("verified automatically"),
        "the not-configured banner must state the auto-verify consequence"
    );
}

#[tokio::test]
async fn the_secrets_at_rest_posture_is_reported_both_ways() {
    let encrypted = render("smtp", true, true).await;
    assert!(encrypted.contains("SECRET_ENCRYPTION_KEY is set"));

    let plaintext = render("smtp", true, false).await;
    assert!(plaintext.contains("plaintext"));
}
