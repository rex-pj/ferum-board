//! Renders the two pages that host `<ferum-image-cropper>` for real.
//!
//! `tera_templates.rs` proves these templates *parse*, which says nothing about
//! whether they render. The cropper is placed inside `{% if %}` branches and its
//! label goes through `t(k=…)`, so the two ways of getting it wrong — a branch
//! that never emits the element, and a catalog key that does not exist — both
//! survive parsing and show up only on the page.
//!
//! The catalog assertion matters more than it looks: Fluent renders an unknown
//! key as the key itself, so a missing entry produces a button reading
//! `ui-crop-thumbnail`. Nothing errors, and nothing in the existing suite
//! notices.

use std::path::PathBuf;

use ferum_domain::Locale;
use ferum_web::view_models::page_context::{SiteCtx, UserProfileCtx};
use serde_json::json;
use tera::Context;

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

/// The page shell every template gets from `render_with_theme_in`.
fn shell() -> Context {
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
    ctx.insert("active_theme", "default");
    ctx.insert("nav_categories", &json!([]));
    ctx.insert("plugin_slots", &json!({}));
    ctx.insert("plugin_assets", &json!([]));
    ctx.insert("theme_bs_theme", "auto");
    ctx.insert("locale", "en");
    ctx.insert("js_strings", &json!({}));
    ctx.insert("available_locales", &json!(["en", "vi"]));
    ctx
}

fn viewer() -> serde_json::Value {
    json!({
        "id": "00000000-0000-0000-0000-00000000a001",
        "username": "kim",
        "display_name": "Kim",
        "avatar_url": null,
        "trust_level": "member",
        "is_admin": false,
        "is_mod": false,
        // Read by `partials/notif_bell.html`, which base.html includes on every
        // signed-in page — so any render test for an app page needs it.
        "unread_count": 0
    })
}

async fn render(template: &str, ctx: Context) -> String {
    engine()
        .await
        .render(&Locale::default_locale(), template, ctx)
        .await
        .unwrap_or_else(|e| panic!("{template} must render: {e:?}"))
}

/// Fluent echoes an unknown key back, so a raw `ui-`/`js-` token in the output
/// is a catalog entry someone forgot to add in one or both locales.
fn assert_no_untranslated_keys(html: &str, page: &str) {
    for token in ["ui-crop-thumbnail", "ui-change-photo", "ui-upload-photo", "ui-change-cover"] {
        assert!(
            !html.contains(&format!(">{token}<")) && !html.contains(&format!("\"{token}\"")),
            "{page} rendered the raw key `{token}` — it is missing from locales/en/common.ftl"
        );
    }
}

#[tokio::test]
async fn account_page_offers_a_cropper_for_both_avatar_and_cover() {
    let mut ctx = shell();
    ctx.insert("current_user", &viewer());
    ctx.insert("current_path", "/account");
    // Built from the real struct, not a hand-written JSON blob: a field renamed
    // on `UserProfileCtx` then breaks this file instead of silently diverging
    // from what the handler actually inserts.
    ctx.insert(
        "profile",
        &UserProfileCtx {
            id: "00000000-0000-0000-0000-00000000a001".into(),
            username: "kim".into(),
            display_name: "Kim".into(),
            avatar_url: None,
            cover_url: None,
            bio: None,
            website: None,
            trust_level: "member".into(),
            trust_score: Some(10),
            primary_role_slug: None,
            primary_role_name: None,
            primary_role_color: None,
            post_count: 12,
            follower_count: 0,
            following_count: 0,
            created_at: "2026-01-05T09:00:00+00:00".into(),
            threads: vec![],
            thread_pagination: None,
        },
    );
    ctx.insert(
        "preferences",
        &json!({ "theme": "auto", "font_size": "medium", "layout": "comfortable",
                 "locale": "en", "email_notifications": {} }),
    );
    ctx.insert("account_status", &json!(null));

    let html = render("default/templates/app/account.html", ctx).await;

    // Two elements, not one: avatar and cover are separate uploads with
    // different endpoints, and a single shared cropper would post both to
    // whichever endpoint happened to be written last.
    assert_eq!(
        html.matches("<ferum-image-cropper").count(),
        2,
        "account page must host a cropper for the avatar and one for the cover"
    );
    assert!(html.contains(r#"endpoint="/api/users/me/avatar""#));
    assert!(html.contains(r#"endpoint="/api/users/me/cover""#));

    // These mirror AVATAR_FRAME and COVER_FRAME in constants.rs. The server
    // crops to those ratios whatever the widget says, so a drifted value here
    // previews one framing and stores another.
    assert!(html.contains(r#"aspect="1""#), "avatar frame must be 1:1");
    assert!(html.contains(r#"aspect="4""#), "cover frame must be 4:1");

    // The old file inputs are gone; leaving one would give two upload paths,
    // only one of which sends a crop rectangle.
    assert!(!html.contains(r#"id="avatar-upload""#));
    assert!(!html.contains(r#"id="cover-upload""#));

    assert_no_untranslated_keys(&html, "account.html");
}

#[tokio::test]
async fn edit_thread_offers_a_cropper_when_the_author_may_upload() {
    let mut ctx = shell();
    ctx.insert("current_user", &viewer());
    ctx.insert("current_path", "/edit-thread/sofa-2y");
    ctx.insert("thread_slug", "sofa-2y");
    ctx.insert("thread_title", "Sofa after 2 years");
    ctx.insert("thread_thumbnail_url", &json!(null));
    ctx.insert("thread_tags", &json!([]));
    ctx.insert("first_post_content", "Still holding up.");
    ctx.insert("thread_category_id", "00000000-0000-0000-0000-0000000000e1");
    ctx.insert("can_upload_thumbnail", &true);
    ctx.insert(
        "categories",
        &json!([{ "id": "00000000-0000-0000-0000-0000000000e1", "name": "General", "slug": "general" }]),
    );

    let html = render("default/templates/app/edit_thread.html", ctx).await;

    assert!(html.contains("<ferum-image-cropper"));
    assert!(html.contains(r#"endpoint="/api/threads/sofa-2y/thumbnail""#));
    // THUMBNAIL_FRAME is 1280×720 — see constants.rs.
    assert!(html.contains(r#"aspect="1.7778""#), "thumbnail frame must be 16:9");
    assert_no_untranslated_keys(&html, "edit_thread.html");
}

#[tokio::test]
async fn edit_thread_hides_the_cropper_from_an_author_who_may_not_upload() {
    // The gate is `can_upload_thumbnail`. Rendering the widget anyway would
    // offer an upload the server then refuses with `trust_level_insufficient`,
    // which reads as a broken feature rather than a permission boundary.
    let mut ctx = shell();
    ctx.insert("current_user", &viewer());
    ctx.insert("current_path", "/edit-thread/sofa-2y");
    ctx.insert("thread_slug", "sofa-2y");
    ctx.insert("thread_title", "Sofa after 2 years");
    ctx.insert("thread_thumbnail_url", &json!(null));
    ctx.insert("thread_tags", &json!([]));
    ctx.insert("first_post_content", "Still holding up.");
    ctx.insert("thread_category_id", "00000000-0000-0000-0000-0000000000e1");
    ctx.insert("can_upload_thumbnail", &false);
    ctx.insert("categories", &json!([]));

    let html = render("default/templates/app/edit_thread.html", ctx).await;

    assert!(
        !html.contains("<ferum-image-cropper"),
        "a user without upload rights must not be offered the cropper"
    );
}
