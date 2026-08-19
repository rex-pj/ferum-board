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
use ferum_domain::site_text::{localized_key, LOCALIZED_CONFIG_KEYS};
use ferum_web::handlers::admin::api::config::{
    is_writable_config_key, CONFIG_SECRET_KEYS, CONFIG_WRITABLE_KEYS,
};
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
/// The stored relay host most tests want. Only the email-switch test cares that
/// this is variable, because the switch is a view over exactly this value.
const HOST: &str = "smtp.example.com";

fn ctx(mail_provider: &str, smtp_pass_set: bool, secrets_encrypted: bool) -> Context {
    ctx_with_host(mail_provider, smtp_pass_set, secrets_encrypted, HOST)
}

fn ctx_with_host(
    mail_provider: &str,
    smtp_pass_set: bool,
    secrets_encrypted: bool,
    smtp_host: &str,
) -> Context {
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
    config.insert("smtp_host".into(), json!(smtp_host));
    config.insert("smtp_port".into(), json!("587"));
    config.insert("smtp_user".into(), json!("apikey"));
    ctx.insert("config", &serde_json::Value::Object(config));
    // `site_copy` — one pane per installed locale, built by the handler from
    // `site_text::localized_key`. Two locales exercise the language tab strip;
    // `a_single_language_install_shows_no_language_tabs` covers the other branch.
    ctx.insert("site_copy", &site_copy_ctx(&["en", "vi"]));
    ctx.insert("smtp_pass_set", &smtp_pass_set);
    // Presence flag, never the key: env-only, and the panel only needs to know
    // whether selecting Resend would send anything.
    ctx.insert("resend_key_present", &(mail_provider == "resend"));
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

async fn render_with_host(mail_provider: &str, smtp_host: &str) -> String {
    engine()
        .await
        .render(
            &Locale::default_locale(),
            TEMPLATE,
            ctx_with_host(mail_provider, true, false, smtp_host),
        )
        .await
        .unwrap_or_else(|e| panic!("rendering {TEMPLATE} with host {smtp_host:?} failed: {e:?}"))
}

/// The `<input …>` tag carrying `id="{id}"` — the plain-id counterpart of
/// [`input_tag`], for controls that are deliberately NOT `cfg-*` fields.
fn tag_by_id<'a>(html: &'a str, id: &str) -> &'a str {
    let needle = format!("id=\"{id}\"");
    let at = html
        .find(&needle)
        .unwrap_or_else(|| panic!("no element with id {id}"));
    let open = html[..at].rfind('<').expect("tag start");
    let close = at + html[at..].find('>').expect("tag end");
    &html[open..=close]
}

#[tokio::test]
async fn the_provider_selection_is_one_hidden_field_two_controls_write() {
    // "Off" is now the stored value `mail_provider = off`, not a blank smtp_host.
    // The switch and the pills both write ONE field, so a save can never carry two
    // disagreeing opinions about which provider was chosen — and the operator can
    // switch mail off without erasing a relay they would have to retype.
    let html = render("smtp", true, false).await;

    let hidden = tag_by_id(&html, "cfg-mail_provider");
    assert!(
        hidden.contains(r#"type="hidden""#),
        "the selection must be a real cfg-* field so saveSettings picks it up: {hidden}"
    );

    // The switch itself must NOT be a cfg-* field: `mail_provider` already carries
    // the fact, and a second key would give it two owners.
    assert!(
        !html.contains("cfg-mail_enabled") && !html.contains("cfg-mail-enabled"),
        "the switch must not masquerade as a config key of its own"
    );

    // Both writers have to be findable by the script that keeps them in sync —
    // and there must be EXACTLY ONE of each.
    //
    // `contains` alone was not enough, and this is not hypothetical: moving the
    // switch out of the SMTP card left a copy behind, so the page rendered two
    // `id="mail-enabled"` inputs. `getElementById` returns the first, so the
    // second was inert while its `<label for>` still toggled the first — a
    // control that visibly does nothing when clicked, and silently moves another
    // one. A substring assertion passes happily on both.
    for id in ["mail-enabled", "mail-provider-pills", "cfg-mail_provider"] {
        assert_eq!(
            html.matches(&format!("id=\"{id}\"")).count(),
            1,
            "exactly one element may carry id={id:?}"
        );
    }
}

#[tokio::test]
async fn the_provider_panel_that_opens_is_the_one_that_is_live() {
    // Each provider owns a panel and the pills switch between them, so the pane
    // that opens has to be the provider mail actually goes through — otherwise the
    // page lands on settings that are not in use, which is the confusion the split
    // was meant to end.
    //
    // BOTH panes always exist, and for different reasons: the SMTP one because
    // unsetting RESEND_API_KEY falls straight back to those stored values, the
    // Resend one because it is the only place that names its env switches.
    for (provider, live_pane) in [
        ("resend", "mail-pane-resend"),
        ("smtp", "mail-pane-smtp"),
        // Nothing configured: SMTP is the pane that can fix it.
        ("disabled", "mail-pane-smtp"),
    ] {
        let html = render(provider, true, false).await;
        for pane in ["mail-pane-resend", "mail-pane-smtp"] {
            let tag = tag_by_id(&html, pane);
            assert_eq!(
                tag.contains("show active"),
                pane == live_pane,
                "provider {provider}: {pane} open-state is wrong ({tag})"
            );
        }
    }
}

#[tokio::test]
async fn example_values_are_marked_as_examples() {
    // A realistic placeholder in an empty field reads as a stored setting, which
    // is how an unconfigured relay managed to look configured. "e.g." is the
    // cheapest thing that removes the ambiguity, so it is worth not losing.
    let html = render_with_host("smtp", "").await;
    for field in ["smtp_host", "smtp_user"] {
        let tag = input_tag(&html, field);
        assert!(
            tag.contains("placeholder=\"e.g."),
            "cfg-{field} shows an example value that is not marked as one: {tag}"
        );
    }
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
async fn the_smtp_password_state_is_carried_by_a_badge_not_the_placeholder() {
    // The affordance the leak fix had to preserve: the page must still be able to
    // say whether a password exists, without holding it. What changed is the
    // CARRIER. A placeholder vanishes the instant the field takes focus — which
    // is exactly when the operator is deciding whether typing here replaces
    // something — and it renders as the faintest text on the form, so state
    // moved to a badge beside the label.
    let set = render("smtp", true, false).await;
    let set_label = label_for(&set, "cfg-smtp_pass");
    assert!(set_label.contains("Saved"), "a stored password must be reported");
    assert!(!set_label.contains("Not set"));

    let unset = render("smtp", false, false).await;
    let unset_label = label_for(&unset, "cfg-smtp_pass");
    assert!(unset_label.contains("Not set"));
    assert!(!unset_label.contains("Saved"));

    // And the input itself must no longer differ between the two states: a
    // placeholder that still branched on the flag would be a second, quieter
    // source of truth for the same fact.
    assert_eq!(
        input_tag(&set, "smtp_pass"),
        input_tag(&unset, "smtp_pass"),
        "the password input must be identical either way now the badge carries state"
    );
}

#[tokio::test]
async fn the_smtp_password_field_is_masked_and_not_autofillable() {
    // It holds an SMTP password or a provider API key and used to render as
    // type=text, legible on screen. `new-password` rather than `off` because
    // Chrome ignores `off` and will offer the operator's own login password.
    let html = render("smtp", true, false).await;
    let tag = input_tag(&html, "smtp_pass");
    assert!(tag.contains(r#"type="password""#), "got: {tag}");
    assert!(tag.contains(r#"autocomplete="new-password""#), "got: {tag}");
}

/// Returns the raw `<input …>` tag carrying `id="cfg-{field}"`.
///
/// Scoped to the tag rather than the tab, because the surrounding help text
/// legitimately contains words like "disable" — this page renders every tab, so
/// a substring search over the whole document passes and fails for the wrong
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

/// Returns the `<label …>…</label>` bound to element `id`, without its closing tag.
/// Scoped for the same reason as [`input_tag`]: "Saved" and "Not set" are short enough
/// to appear in unrelated copy on one of the other tabs.
///
/// Takes the full id, not a `cfg-` suffix — not every labelled control on this page is
/// a config field (`primary-color-hex` is a view over one).
fn label_for<'a>(html: &'a str, id: &str) -> &'a str {
    let needle = format!("for=\"{id}\"");
    let at = html
        .find(&needle)
        .unwrap_or_else(|| panic!("no label bound to {id}"));
    let open = html[..at].rfind("<label").expect("label tag start");
    let close = at + html[at..].find("</label>").expect("label tag end");
    &html[open..close]
}

const SMTP_FIELDS: [&str; 4] = ["smtp_host", "smtp_port", "smtp_user", "smtp_pass"];

#[tokio::test]
async fn the_smtp_fields_stay_editable_under_every_provider() {
    // **This inverts the previous contract, deliberately.** They used to be
    // rendered `disabled` whenever Resend was live, which made sense only while
    // the provider was decided by an environment variable: there was nothing an
    // operator could do about it from this page anyway.
    //
    // Now that the provider is a stored setting, disabling them makes the panel
    // unusable in exactly the situation it is needed — you configure a relay
    // *before* switching to it, and a disabled field cannot be filled in. Nor is
    // there anything left to protect: `mail_reload_from_config` builds whichever
    // provider is selected regardless of what else is stored.
    for provider in ["resend", "smtp", "disabled"] {
        let html = render(provider, true, false).await;
        for field in SMTP_FIELDS {
            let tag = input_tag(&html, field);
            assert!(
                !tag.contains("disabled"),
                "cfg-{field} must stay editable when the live provider is {provider}: {tag}"
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

/// `site_copy` as the settings handler builds it: one pane per installed locale,
/// source locale first, each holding a field per localized key.
///
/// Keys go through `site_text::localized_key` rather than being spelled out, so this
/// fixture cannot drift from the rule the handler and the writability check share.
fn site_copy_ctx(tags: &[&str]) -> serde_json::Value {
    let panes: Vec<serde_json::Value> = tags
        .iter()
        .map(|tag| {
            let locale = Locale::parse(tag).expect("test tags are valid");
            let is_source = locale.is_source_locale();
            let fields: serde_json::Map<String, serde_json::Value> = LOCALIZED_CONFIG_KEYS
                .iter()
                .map(|base| {
                    (
                        (*base).to_string(),
                        json!({
                            "key": localized_key(base, &locale),
                            "value": "",
                            // The source pane has no fallback of its own; a
                            // translation pane shows the source text as placeholder.
                            "fallback": if is_source { "" } else { FALLBACK_HINT },
                        }),
                    )
                })
                .collect();
            json!({
                "tag": tag,
                "name": tag,
                "is_source": is_source,
                "fields": serde_json::Value::Object(fields),
            })
        })
        .collect();
    json!(panes)
}

/// Stands in for the source-locale copy on a translation pane's placeholder.
const FALLBACK_HINT: &str = "a modern self-hosted forum";
/// Every `cfg-*` id the *rendered* page carries, in document order.
fn rendered_cfg_ids(html: &str) -> Vec<&str> {
    html.match_indices("id=\"cfg-")
        .map(|(at, _)| {
            let rest = &html[at + "id=\"cfg-".len()..];
            let end = rest.find('"').expect("unterminated id attribute");
            &rest[..end]
        })
        .collect()
}

#[tokio::test]
async fn every_rendered_cfg_field_is_writable() {
    // `settings_template_contract.rs` scans the template source, which cannot see
    // through `cfg-{{ f.key }}` in the copy_field macro. Only a real render can, and
    // the failure it guards against is the worst kind available here: a translation
    // field whose key `update_config` drops, so the save returns 200 and changes
    // nothing.
    let html = render("smtp", true, false).await;
    let ids = rendered_cfg_ids(&html);

    assert!(
        ids.contains(&"site_tagline:vi"),
        "the language panes rendered no per-locale field; ids were {ids:?}"
    );
    for id in &ids {
        assert!(
            is_writable_config_key(id),
            "cfg-{id} is rendered but `update_config` would drop it"
        );
    }

    // The translation input must carry the same cap as the field it translates, or
    // one locale can be typed longer than another and only that save fails.
    assert_eq!(
        length_cap(&html, "site_tagline:vi"),
        length_cap(&html, "site_tagline"),
        "a translation field's maxlength differs from the field it translates"
    );
}

#[tokio::test]
async fn a_label_passed_into_the_macro_survives() {
    // A Tera macro does NOT inherit the render context, so a label the macro looked up
    // itself would render empty. These are passed in as arguments; the failure mode is
    // a field with no visible label at all, which no other test would notice.
    let html = render("smtp", true, false).await;
    for label in ["Slogan", "Tagline"] {
        assert!(html.contains(label), "the copy field lost its {label} label");
    }
}

#[tokio::test]
async fn a_translation_pane_offers_the_fallback_text_as_its_placeholder() {
    // A blank translation falls back to the source copy, and the placeholder is where
    // that is visible — an empty field with no hint reads as "this language has no
    // tagline", which is the opposite of what happens.
    let html = render("smtp", true, false).await;
    let translated = input_tag(&html, "site_tagline:vi");
    assert!(
        translated.contains(&format!("placeholder=\"{FALLBACK_HINT}\"")),
        "the translation field does not show the fallback copy: {translated}"
    );
    // …and the source field keeps its own example placeholder rather than echoing
    // itself.
    let source = input_tag(&html, "site_tagline");
    assert!(
        !source.contains(FALLBACK_HINT),
        "the source field is using the fallback hint as a placeholder: {source}"
    );
}

#[tokio::test]
async fn every_installed_language_gets_a_tab_and_exactly_one_pane_is_open() {
    // Two panes open at once stacks both languages' fields on top of each other; none
    // open leaves the card looking empty. Bootstrap picks the pane by `show active`,
    // so that has to land on exactly one — the first, which is the source locale.
    let html = render("smtp", true, false).await;
    for tag in ["en", "vi"] {
        assert_eq!(
            html.matches(&format!("id=\"copy-pane-{tag}\"")).count(),
            1,
            "expected exactly one pane for {tag}"
        );
        assert!(
            html.contains(&format!("data-bs-target=\"#copy-pane-{tag}\"")),
            "no language tab targets the {tag} pane"
        );
    }
    assert_eq!(
        html.matches("copy-pane-").count(),
        // one button target + one pane id per locale, and one `show active` marker
        // that is counted by neither.
        4,
        "the language tab strip and the panes are out of step"
    );
}

#[tokio::test]
async fn a_single_language_install_shows_no_language_tabs() {
    // One installed language means there is nothing to switch between, and a lone
    // pill labelled "English" beside a "Fallback" badge is noise that implies a
    // choice the operator does not have.
    let mut c = ctx("smtp", true, false);
    c.insert("site_copy", &site_copy_ctx(&["en"]));
    let html = engine()
        .await
        .render(&Locale::default_locale(), TEMPLATE, c)
        .await
        .expect("rendering a single-language install must succeed");

    for id in rendered_cfg_ids(&html) {
        assert!(
            !id.contains(':'),
            "cfg-{id} is a per-locale field on a single-language install"
        );
    }
    assert!(
        !html.contains("adm-copy-langs"),
        "the language tab strip must not render for a single language"
    );
    // …nor the rule explaining a fallback there is nothing to fall back from.
    assert!(
        !html.contains("Fallback where its own text is blank"),
        "the fallback rule must not render for a single language"
    );
    // The fields themselves must still be there — hiding the strip must not hide the
    // pane it switches.
    assert!(
        html.contains("id=\"cfg-site_tagline\""),
        "the source copy fields disappeared with the tab strip"
    );
}

/// The `maxlength` on `cfg-{field}`.
fn length_cap<'a>(html: &'a str, field: &str) -> &'a str {
    let tag = input_tag(html, field);
    let at = tag
        .find("maxlength=\"")
        .unwrap_or_else(|| panic!("cfg-{field} has no maxlength: {tag}"));
    let rest = &tag[at + "maxlength=\"".len()..];
    &rest[..rest.find('"').expect("unterminated maxlength")]
}


#[tokio::test]
async fn the_fallback_rule_is_stated_once_beside_the_language_pills() {
    // It says the same thing whichever pane is open, so it belongs to the section, not
    // to a pane. Repeating it per pane also made the card resize on every switch — the
    // source pane's wording was a line shorter, so the fields jumped under the pointer.
    let html = render("smtp", true, false).await;
    assert_eq!(
        html.matches("Fallback where its own text is blank").count(),
        1,
        "the fallback rule must be stated exactly once"
    );
}

#[tokio::test]
async fn slogan_and_tagline_share_a_row_in_every_language() {
    // The two are read together and written together, so they sit side by side rather
    // than stacked. Asserted between the two ids so it holds per language, rather than
    // wherever a `col-md-6` happens to appear elsewhere on the page.
    let html = render("smtp", true, false).await;
    for suffix in ["", ":vi"] {
        let from = html
            .find(&format!("cfg-site_slogan{suffix}\""))
            .unwrap_or_else(|| panic!("no slogan field for {suffix:?}"));
        let to = html
            .find(&format!("cfg-site_tagline{suffix}\""))
            .unwrap_or_else(|| panic!("no tagline field for {suffix:?}"));
        assert!(from < to, "the fields are out of order for {suffix:?}");
        assert!(
            html[from..to].contains("col-md-6"),
            "slogan and tagline are not half-width siblings for {suffix:?}"
        );
    }
}

#[tokio::test]
async fn the_accent_colour_sits_inside_the_branding_grid() {
    // It used to be a full-width row of its own below the two upload columns, which
    // left two voids at once: the favicon column ended a block early, and a 220px
    // control had the rest of a 12-wide row to itself. Placing it in the shorter column
    // closes both. Pinned because the row it came from is one edit away from returning.
    let html = render("smtp", true, false).await;

    let color = html
        .find("id=\"cfg-primary_color\"")
        .expect("the colour control must render");
    let favicon = html
        .find("id=\"favicon-file-input\"")
        .expect("the favicon upload must render");
    // The next card. Everything between the favicon upload and it is still Branding.
    let next_card = html
        .find("id=\"cfg-reporting_timezone\"")
        .expect("the Localization card must follow Branding");

    assert!(
        favicon < color && color < next_card,
        "the colour control is not inside the Branding card, after the favicon block"
    );
    // The full-width row it came from was introduced by an `<hr>`. Its absence between
    // the two is what says the colour is a cell in the grid rather than a row below it.
    assert!(
        !html[favicon..color].contains("<hr"),
        "an <hr> between the favicon block and the colour means the colour is back in a \
         full-width row of its own"
    );
}

#[tokio::test]
async fn the_two_second_row_branding_blocks_are_built_identically() {
    // Logo URL and the accent colour sit opposite each other as each column's second
    // block, so they have to start on the same line. That is purely a function of their
    // wrapper and label classes being the same, and they had already drifted — one
    // bordered and one not, one `form-control-sm` and one not — which reads as a
    // misaligned card at a glance and is invisible to every other test here.
    let html = render("smtp", true, false).await;

    for field in ["cfg-logo_url", "primary-color-hex"] {
        let label = label_for(&html, field);
        assert!(
            label.contains("adm-section-label") && label.contains("text-uppercase"),
            "cfg-{field}'s block label is not the shared section-label style: {label}"
        );
        assert!(
            html[..html.find(&format!("id=\"{field}\"")).expect("field renders")]
                .rfind("border-top pt-3 mt-4")
                .is_some_and(|at| at > html.find("class=\"row g-4\"").expect("branding row")),
            "cfg-{field}'s block is not opened by the shared `border-top pt-3 mt-4` wrapper"
        );
    }

    // Neither may be a small control while the other is full size.
    assert!(
        !input_tag(&html, "logo_url").contains("form-control-sm"),
        "the logo URL input is a size the accent colour beside it is not"
    );
}
