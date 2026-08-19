//! The URL shape `base.html` advertises to crawlers — `hreflang` and `rel=canonical`.
//!
//! `hreflang` has to name, for each installed language, the URL that serves it —
//! and exactly one of them is served **unprefixed**: the site default. That used to
//! be a literal `'en'` in the template, so a forum whose default was Vietnamese told
//! every crawler that `/` was English and that Vietnamese lived at `/vi/…`, which is
//! the reverse of what the server does. Nothing failed; the pages rendered, and the
//! wrong claim only ever surfaced in a search index.
//!
//! Rendering is the only way to check this. `tera_templates.rs` proves the template
//! parses, and a parse says nothing about which branch a comparison takes.

use std::path::PathBuf;

use ferum_domain::Locale;
use ferum_web::view_models::page_context::SiteCtx;
use serde_json::json;
use tera::Context;

const ORIGIN: &str = "https://forum.example.com";

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

/// The login page's context, minus the two variables each test varies.
///
/// The login page, because it is the leanest template that still extends the
/// default `base.html` — the shell is what carries `hreflang`, and every richer page
/// would drag in feed and catalogue context this test has no opinion about.
fn ctx(default_locale: &str, installed: &[&str]) -> Context {
    let mut c = Context::new();
    c.insert(
        "site",
        &SiteCtx {
            name: "Ferum".into(),
            slogan: String::new(),
            tagline: "Furniture reviews".into(),
            logo_url: None,
            favicon_url: None,
            primary_color: None,
            primary_color_rgb: None,
            url: ORIGIN.into(),
        },
    );
    c.insert("current_user", &json!(null));
    c.insert("active_theme", "default");
    c.insert("nav_categories", &json!([]));
    c.insert("error", &json!(null));
    c.insert("plugin_slots", &json!({}));
    c.insert("plugin_assets", &json!([]));
    c.insert("theme_bs_theme", "auto");
    c.insert("locale", default_locale);
    c.insert("default_locale", default_locale);
    c.insert("current_path", "/forum");
    c.insert("js_strings", &json!({}));
    c.insert("available_locales", &json!(installed));
    c
}

async fn render(default_locale: &str, installed: &[&str]) -> String {
    engine()
        .await
        .render(
            &Locale::default_locale(),
            "default/templates/auth/login.html",
            ctx(default_locale, installed),
        )
        .await
        .expect("login.html must render")
}

/// The `href` of the `<link rel="alternate">` for `tag`, with Tera's attribute
/// escaping undone.
///
/// Tera escapes `/` to `&#x2F;`, which browsers and crawlers decode — the same
/// harmless escaping `file_url` produces. Decoding here keeps the assertions
/// readable as the URLs they actually are.
fn alternate(html: &str, tag: &str) -> String {
    let needle = format!("hreflang=\"{tag}\"");
    let at = html
        .find(&needle)
        .unwrap_or_else(|| panic!("no alternate for {tag}"));
    let href = at + html[at..].find("href=\"").expect("alternate has an href")
        + "href=\"".len();
    html[href..href + html[href..].find('"').expect("unterminated href")]
        .replace("&#x2F;", "/")
}

#[tokio::test]
async fn the_site_default_is_the_unprefixed_url() {
    // With `vi` as the site default, `/forum` serves Vietnamese and English lives at
    // `/en/forum` — the exact inverse of what the hardcoded template claimed.
    let html = render("vi", &["en", "vi"]).await;
    assert_eq!(alternate(&html, "vi"), format!("{ORIGIN}/forum"));
    assert_eq!(alternate(&html, "en"), format!("{ORIGIN}/en/forum"));
}

#[tokio::test]
async fn english_default_keeps_the_shape_it_had() {
    // The overwhelmingly common case must be untouched by making the setting real.
    let html = render("en", &["en", "vi"]).await;
    assert_eq!(alternate(&html, "en"), format!("{ORIGIN}/forum"));
    assert_eq!(alternate(&html, "vi"), format!("{ORIGIN}/vi/forum"));
}

#[tokio::test]
async fn x_default_always_points_at_the_unprefixed_url() {
    // `x-default` is what a crawler serves to a language it has no better match for,
    // so it must be the URL that negotiates rather than one that pins a language.
    for default_locale in ["en", "vi"] {
        let html = render(default_locale, &["en", "vi"]).await;
        assert_eq!(
            alternate(&html, "x-default"),
            format!("{ORIGIN}/forum"),
            "x-default is wrong when the site default is {default_locale}"
        );
    }
}

#[tokio::test]
async fn a_single_language_site_advertises_no_alternates() {
    // One language means there is nothing to alternate between, and a lone
    // self-referential hreflang is noise in every crawler's index.
    let html = render("en", &["en"]).await;
    assert!(
        !html.contains("hreflang"),
        "a single-language site must emit no alternates"
    );
}

/// The `href` of the page's `<link rel="canonical">`.
fn canonical(html: &str) -> String {
    let at = html
        .find("rel=\"canonical\"")
        .expect("the page must carry a canonical URL");
    let href = at + html[at..].find("href=\"").expect("canonical has an href") + "href=\"".len();
    html[href..href + html[href..].find('"').expect("unterminated href")].replace("&#x2F;", "/")
}

/// [`render`], with the resolved locale set independently of the site default.
async fn render_as(locale: &str, default_locale: &str, path: &str) -> String {
    let mut c = ctx(default_locale, &["en", "vi"]);
    c.insert("locale", locale);
    c.insert("current_path", path);
    engine()
        .await
        .render(
            &Locale::default_locale(),
            "default/templates/auth/login.html",
            c,
        )
        .await
        .expect("login.html must render")
}

#[tokio::test]
async fn the_prefixed_and_unprefixed_forms_of_one_language_share_a_canonical() {
    // The site default is reachable both ways — `/forum` and `/vi/forum` when `vi` is
    // the default — and nothing before this told a crawler which to keep. Both must
    // name the unprefixed one, or the two compete as duplicates.
    let html = render_as("vi", "vi", "/forum").await;
    assert_eq!(canonical(&html), format!("{ORIGIN}/forum"));
    // The same holds for the other direction: with `en` as the default, `/en/forum`
    // canonicalises to `/forum`.
    let html = render_as("en", "en", "/forum").await;
    assert_eq!(canonical(&html), format!("{ORIGIN}/forum"));
}

#[tokio::test]
async fn a_non_default_language_canonicalises_to_its_own_prefix() {
    // A translation is not a duplicate of the default language. Pointing it at the
    // unprefixed URL would ask a crawler to drop it from the index entirely.
    let html = render_as("en", "vi", "/forum").await;
    assert_eq!(canonical(&html), format!("{ORIGIN}/en/forum"));

    let html = render_as("vi", "en", "/forum").await;
    assert_eq!(canonical(&html), format!("{ORIGIN}/vi/forum"));
}

#[tokio::test]
async fn a_paginated_page_canonicalises_to_itself() {
    // `current_path` carries `?page=N` for N > 1, and the canonical has to keep it:
    // naming page 1 here would tell a crawler every later page is a duplicate, so the
    // threads that appear only on them leave the index.
    let html = render_as("en", "en", "/forum?page=4").await;
    assert_eq!(canonical(&html), format!("{ORIGIN}/forum?page=4"));

    let html = render_as("vi", "en", "/forum?page=4").await;
    assert_eq!(canonical(&html), format!("{ORIGIN}/vi/forum?page=4"));
}

#[tokio::test]
async fn exactly_one_canonical_is_emitted() {
    // Two canonical tags are treated as none at all by every major crawler, which is
    // the failure mode of a page overriding the block and adding a tag beside it
    // instead of replacing it.
    let html = render_as("en", "en", "/forum").await;
    assert_eq!(
        html.matches("rel=\"canonical\"").count(),
        1,
        "a page must carry exactly one canonical URL"
    );
}

#[tokio::test]
async fn the_canonical_and_the_self_alternate_agree() {
    // `hreflang` names this page in every language and `rel=canonical` names this one,
    // so the alternate for the current locale and the canonical must be the same URL.
    // They are built by separate expressions, which is exactly how they drift.
    for (locale, default_locale) in [("en", "en"), ("vi", "en"), ("en", "vi"), ("vi", "vi")] {
        let html = render_as(locale, default_locale, "/forum").await;
        assert_eq!(
            canonical(&html),
            alternate(&html, locale),
            "canonical and the {locale} alternate disagree (site default {default_locale})"
        );
    }
}
