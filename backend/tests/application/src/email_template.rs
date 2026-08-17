//! Tests for the email template renderer's pure half.
//!
//! Two of these guard a security property rather than a behaviour: that a body
//! escapes per the variable's declared kind, and that a subject does not escape
//! at all. Getting either backwards is invisible in a passing build — one puts
//! markup in a stranger's inbox, the other shows `&amp;` in an inbox list.

use std::collections::HashMap;

use ferum_application::email_template::{
    escape_html, html_to_text, placeholders, sanitize_url, substitute, validate_template,
};
use ferum_domain::models::email_template::{
    template_def, template_default, VarDef, VarKind, EMAIL_TEMPLATES, LAYOUT_KEY,
};

fn values(pairs: &[(&'static str, &str)]) -> HashMap<&'static str, String> {
    pairs.iter().map(|(k, v)| (*k, v.to_string())).collect()
}

const DEFS: &[VarDef] = &[
    VarDef { name: "text", kind: VarKind::Text, required_in_body: false },
    VarDef { name: "link", kind: VarKind::Url, required_in_body: false },
    VarDef { name: "raw", kind: VarKind::Raw, required_in_body: false },
];

// ─── Substitution and escaping ────────────────────────────────────────────────

#[test]
fn a_body_escapes_text_but_not_a_subject() {
    let v = values(&[("text", "Bells & <b>Whistles</b>")]);

    let body = substitute("<p>{{ text }}</p>", &v, DEFS, true);
    assert_eq!(body, "<p>Bells &amp; &lt;b&gt;Whistles&lt;/b&gt;</p>");

    let subject = substitute("{{ text }}", &v, DEFS, false);
    assert_eq!(
        subject, "Bells & <b>Whistles</b>",
        "a subject is plain text; escaping shows entities in the inbox list"
    );
}

#[test]
fn a_url_variable_rejects_a_scheme_that_would_execute() {
    let v = values(&[("link", "javascript:alert(1)")]);
    let body = substitute("<a href=\"{{ link }}\">go</a>", &v, DEFS, true);
    assert_eq!(
        body, "<a href=\"#\">go</a>",
        "only http(s) may reach an href, and the link stays visible so the fault is reportable"
    );
}

#[test]
fn a_url_variable_keeps_a_normal_link_usable() {
    let v = values(&[("link", "https://example.test/a?x=1&y=2")]);
    let body = substitute("<a href=\"{{ link }}\">go</a>", &v, DEFS, true);
    assert!(body.contains("https://example.test/a?x=1&amp;y=2"), "got: {body}");
}

#[test]
fn a_raw_variable_is_inserted_verbatim() {
    let v = values(&[("raw", "<p>already rendered</p>")]);
    let out = substitute("{{ raw }}", &v, DEFS, true);
    assert_eq!(
        out, "<p>already rendered</p>",
        "the layout's content slot must not be escaped a second time"
    );
}

/// The failure mode this prevents is a blank space where a link belonged.
#[test]
fn an_unsupplied_placeholder_stays_visible() {
    let out = substitute("before {{ text }} after", &HashMap::new(), DEFS, true);
    assert_eq!(out, "before {{ text }} after");
}

#[test]
fn an_unclosed_placeholder_does_not_swallow_the_rest() {
    let v = values(&[("text", "x")]);
    let out = substitute("keep me {{ text", &v, DEFS, true);
    assert_eq!(out, "keep me {{ text");
}

#[test]
fn whitespace_inside_a_placeholder_is_tolerated() {
    let v = values(&[("text", "ok")]);
    assert_eq!(substitute("{{text}}", &v, DEFS, true), "ok");
    assert_eq!(substitute("{{   text   }}", &v, DEFS, true), "ok");
}

/// `&` must be replaced first, or every entity this produces gets its own
/// ampersand escaped a second time.
#[test]
fn escape_html_covers_the_quote_forms() {
    assert_eq!(
        escape_html(r#"<a href="x">'&'</a>"#),
        "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
    );
}

#[test]
fn sanitize_url_accepts_only_http_schemes() {
    assert_eq!(sanitize_url("https://ok.test"), "https://ok.test");
    assert_eq!(sanitize_url("http://ok.test"), "http://ok.test");
    assert_eq!(sanitize_url("  HTTPS://Ok.test  "), "HTTPS://Ok.test");
    assert_eq!(sanitize_url("javascript:x"), "#");
    assert_eq!(sanitize_url("data:text/html,x"), "#");
    assert_eq!(sanitize_url("/relative"), "#");
}

#[test]
fn placeholders_are_listed_once_in_source_order() {
    let found = placeholders("{{ b }} {{ a }} {{ b }}");
    assert_eq!(found, vec!["b".to_string(), "a".to_string()]);
}

// ─── Plain-text alternative ───────────────────────────────────────────────────

#[test]
fn plain_text_keeps_the_link_target() {
    let text = html_to_text(r#"<p>Hi</p><p><a href="https://x.test/t">Read the reply</a></p>"#);
    assert!(
        text.contains("Read the reply (https://x.test/t)"),
        "a text part that drops the link is worse than none; got: {text}"
    );
}

#[test]
fn plain_text_does_not_repeat_a_bare_url_twice() {
    let text = html_to_text(r#"<a href="https://x.test/v">https://x.test/v</a>"#);
    assert_eq!(text, "https://x.test/v");
}

#[test]
fn plain_text_decodes_what_the_body_escaped() {
    let text = html_to_text("<p>Bells &amp; &lt;Whistles&gt;</p>");
    assert_eq!(text, "Bells & <Whistles>");
}

#[test]
fn plain_text_has_no_tags_left() {
    let html = "<p><strong>a</strong> b</p><hr><p style=\"color:#666\">c</p>";
    let text = html_to_text(html);
    assert!(!text.contains('<') && !text.contains('>'), "got: {text}");
}

// ─── Save-time validation ─────────────────────────────────────────────────────

#[test]
fn a_template_may_not_carry_logic() {
    let err = validate_template("email-verify", "s", "{% if x %}{{ url }}{% endif %}")
        .expect_err("`{%` must be refused");
    assert!(format!("{err:?}").contains("logic_unsupported"), "got: {err:?}");
}

#[test]
fn a_template_may_not_name_a_variable_it_has_no_value_for() {
    let err = validate_template("email-verify", "s", "<p>{{ url }} {{ thread_title }}</p>")
        .expect_err("an unavailable variable must be refused");
    assert!(format!("{err:?}").contains("unknown_variable"), "got: {err:?}");
}

/// The rule that keeps notification mail out of spam folders.
#[test]
fn a_notification_body_without_an_unsubscribe_link_is_refused() {
    let err = validate_template("email-notify-reply", "s", "<p><a href=\"{{ url }}\">x</a></p>")
        .expect_err("a missing unsubscribe link must be refused");
    assert!(format!("{err:?}").contains("missing_variable"), "got: {err:?}");
}

#[test]
fn the_shipped_defaults_all_pass_their_own_validation() {
    for t in EMAIL_TEMPLATES {
        for locale in ["en", "vi"] {
            let Some(d) = template_default(t.key, locale) else {
                continue;
            };
            validate_template(d.key, d.subject, d.body_html).unwrap_or_else(|e| {
                panic!("shipped default {} / {} fails validation: {e:?}", d.key, locale)
            });
        }
    }
}

// ─── The guarantee moved here from the Fluent error-catalog test ──────────────

/// Every declared template must render to real copy, never to a raw key.
///
/// Emails are composed by a detached worker with no request context, so a
/// template with no shipped default would reach an inbox as a subject line
/// reading `email-verify`.
#[test]
fn every_template_renders_real_copy_in_the_default_locale() {
    for t in EMAIL_TEMPLATES {
        let d = template_default(t.key, "en")
            .unwrap_or_else(|| panic!("{} ships no en default", t.key));
        let def = template_def(t.key).expect("declared templates are in the catalogue");

        let supplied: HashMap<&str, String> = def
            .vars
            .iter()
            .map(|v| {
                let value = match v.kind {
                    VarKind::Url => "https://example.test/x".to_string(),
                    _ => format!("<{}>", v.name),
                };
                (v.name, value)
            })
            .collect();

        let subject = substitute(d.subject, &supplied, def.vars, false);
        let body = substitute(d.body_html, &supplied, def.vars, true);

        assert_ne!(subject, t.key, "{} rendered its own key as a subject", t.key);
        assert!(!subject.is_empty(), "{} rendered an empty subject", t.key);
        assert!(
            !body.contains("{{"),
            "{} left an unsubstituted placeholder: {body}",
            t.key
        );
        if t.key != LAYOUT_KEY {
            assert!(
                !body.contains("<thread_title>") && !body.contains("<actor>"),
                "{} inserted a Text variable without escaping it: {body}",
                t.key
            );
        }
    }
}
