//! `TeraEngine::build_tera` policy and the i18n Tera bindings, against a
//! synthetic template tree.
//!
//! Synthetic on purpose: the fail-closed half of the policy cannot be reached
//! through the real repository tree without checking a broken template into it.
//! [`super::tera_templates`] covers the real tree instead, so the two are
//! complements — the coverage of "the shipped templates parse" lives there and
//! is not repeated here.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use ferum_application::ports::{TransArg, Translator};
use ferum_domain::Locale;
use ferum_web::tera_engine::TeraEngine;
use tera::{Context, Tera};

/// Renders `<locale>:<key>(+args)` so a test can assert both that `t()` was
/// wired and *which* locale's instance ran, without needing real catalogs.
struct StubTranslator;

#[async_trait::async_trait]
impl Translator for StubTranslator {
    fn translate(&self, locale: &Locale, key: &str, args: &[(&str, TransArg)]) -> String {
        let mut pairs: Vec<String> = args
            .iter()
            .map(|(name, value)| {
                // Rendered without quotes so the assertion is not entangled
                // with Tera's HTML escaping of `"`.
                let rendered = match value {
                    TransArg::Str(s) => format!("Str({s})"),
                    TransArg::Int(i) => format!("Int({i})"),
                    TransArg::Float(f) => format!("Float({f})"),
                };
                format!("|{name}={rendered}")
            })
            .collect();
        pairs.sort();
        format!("{locale}:{key}{}", pairs.concat())
    }
    fn has_key(&self, _locale: &Locale, _key: &str) -> bool {
        true
    }
    fn available_locales(&self) -> Vec<Locale> {
        vec![Locale::default_locale()]
    }
    fn default_locale_keys(&self) -> Vec<String> {
        Vec::new()
    }
    fn js_strings(
        &self,
        _locale: &ferum_domain::Locale,
    ) -> std::sync::Arc<std::collections::BTreeMap<String, String>> {
        // These stubs exist to drive template rendering, not i18n; the `js-`
        // dictionary only reaches the <meta> tag, which no test here reads.
        Default::default()
    }
    async fn reload(&self) -> std::result::Result<(), ferum_application::shared::AppError> {
        Ok(())
    }
}

/// Scratch tree: `<tmp>/<label>-<nonce>/{themes,templates,static}`.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let nonce = format!(
            "{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed),
            label
        );
        let root = std::env::temp_dir().join(format!("ferum-tera-{nonce}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("themes")).unwrap();
        std::fs::create_dir_all(root.join("templates")).unwrap();
        std::fs::create_dir_all(root.join("static")).unwrap();
        Self(root)
    }

    fn write(&self, rel: &str, body: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn build(&self) -> Result<Tera> {
        self.build_in(&Locale::default_locale())
    }

    fn build_in(&self, locale: &Locale) -> Result<Tera> {
        let translator: Arc<dyn Translator> = Arc::new(StubTranslator);
        TeraEngine::build_tera(
            &self.0.join("themes"),
            &self.0.join("templates"),
            &self.0.join("static"),
            locale,
            &translator,
        )
    }

    /// Minimal well-formed `default` theme; every test needs one.
    fn with_default_theme(self) -> Self {
        self.write("themes/default/templates/base.html", "<html>{% block content %}{% endblock %}</html>");
        self.write("themes/default/templates/home.html", "{% extends \"default/templates/base.html\" %}");
        self
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn healthy_tree_loads_everything() {
    let s = Scratch::new("healthy").with_default_theme();
    s.write("templates/admin/base.html", "<html>{% block b %}{% endblock %}</html>");

    let tera = s.build().expect("a healthy tree must build");
    assert!(tera.get_template("default/templates/home.html").is_ok());
    assert!(tera.get_template("admin/base.html").is_ok());
}

/// First-party admin/mod templates fail CLOSED: the app must refuse to boot
/// rather than serve a 500 from a page nobody noticed was broken.
#[test]
fn broken_admin_template_is_a_hard_error() {
    let s = Scratch::new("bad-admin").with_default_theme();
    s.write("templates/admin/broken.html", "{% if x %}never closed");

    let err = s.build().expect_err("a broken admin template must abort the build");
    let msg = err.to_string();
    assert!(msg.contains("admin/broken.html"), "error must name the file, got: {msg}");
}

/// The built-in `default` theme is first-party too, and nothing can fall back
/// to it, so it fails CLOSED as well.
#[test]
fn broken_default_theme_is_a_hard_error() {
    let s = Scratch::new("bad-default");
    s.write("themes/default/templates/base.html", "{% for a in b %}unterminated");

    let err = s.build().expect_err("a broken default theme must abort the build");
    assert!(
        err.to_string().contains("default"),
        "error must mention the default theme, got: {err}"
    );
}

/// A user-uploaded theme fails OPEN: it is skipped, the app still boots, and
/// crucially the healthy themes around it stay registered.
#[test]
fn broken_user_theme_is_skipped_without_taking_down_the_site() {
    let s = Scratch::new("bad-user-theme").with_default_theme();
    s.write("templates/admin/base.html", "<html></html>");
    s.write("themes/aaa-broken/templates/base.html", "{% if nope %}unterminated");
    s.write("themes/zzz-good/templates/base.html", "<html>good</html>");

    let tera = s.build().expect("a broken user theme must not abort the build");

    assert!(
        tera.get_template("aaa-broken/templates/base.html").is_err(),
        "the broken theme must not be registered, not even partially"
    );
    // `aaa-broken` sorts before `zzz-good`: with a single shared batch the bad
    // theme used to abort the load and take this one down with it.
    assert!(
        tera.get_template("zzz-good/templates/base.html").is_ok(),
        "a healthy theme must survive a broken sibling"
    );
    assert!(
        tera.get_template("default/templates/home.html").is_ok(),
        "the default theme must survive a broken user theme"
    );
}

/// Themes may extend other themes; slug order says nothing about that, so the
/// loader retries until it stops making progress.
#[test]
fn child_theme_loads_even_when_it_sorts_before_its_parent() {
    let s = Scratch::new("child-first").with_default_theme();
    // "aaa-child" is visited before "zzz-parent" it extends.
    s.write("themes/zzz-parent/templates/base.html", "<html>{% block c %}{% endblock %}</html>");
    s.write(
        "themes/aaa-child/templates/base.html",
        "{% extends \"zzz-parent/templates/base.html\" %}{% block c %}hi{% endblock %}",
    );

    let tera = s.build().expect("build must succeed");
    assert!(tera.get_template("zzz-parent/templates/base.html").is_ok());
    assert!(
        tera.get_template("aaa-child/templates/base.html").is_ok(),
        "child theme must load on a later pass once its parent is present"
    );
}

/// A theme extending a parent that does not exist can never load; the retry
/// loop must terminate and skip it rather than spin.
#[test]
fn theme_with_missing_parent_is_skipped_and_loop_terminates() {
    let s = Scratch::new("missing-parent").with_default_theme();
    s.write(
        "themes/orphan/templates/base.html",
        "{% extends \"ghost/templates/base.html\" %}",
    );

    let tera = s.build().expect("build must succeed");
    assert!(tera.get_template("orphan/templates/base.html").is_err());
    assert!(tera.get_template("default/templates/home.html").is_ok());
}

#[test]
fn t_function_is_available_to_templates() {
    let s = Scratch::new("t-fn").with_default_theme();
    s.write("themes/default/templates/p.html", r#"{{ t(k="hello-world") }}"#);

    let tera = s.build().unwrap();
    let out = tera
        .render("default/templates/p.html", &Context::new())
        .unwrap();
    assert_eq!(out, "en:hello-world");
}

#[test]
fn t_binds_the_locale_of_its_own_instance() {
    // The core of the per-locale design: the same template text resolves
    // against a different catalog depending on which instance renders it.
    let s = Scratch::new("t-locale").with_default_theme();
    s.write("themes/default/templates/p.html", r#"{{ t(k="greeting") }}"#);

    let vi = Locale::parse("vi").unwrap();
    let out = s
        .build_in(&vi)
        .unwrap()
        .render("default/templates/p.html", &Context::new())
        .unwrap();
    assert_eq!(out, "vi:greeting");
}

#[test]
fn t_forwards_extra_arguments_and_keeps_numbers_numeric() {
    // A stringified count would collapse every Fluent plural rule to its
    // catch-all arm, so the Int/Str distinction has to survive the Tera hop.
    let s = Scratch::new("t-args").with_default_theme();
    s.write(
        "themes/default/templates/p.html",
        r#"{{ t(k="replies", count=5, who="bo") }}"#,
    );

    let out = s
        .build()
        .unwrap()
        .render("default/templates/p.html", &Context::new())
        .unwrap();
    assert_eq!(out, "en:replies|count=Int(5)|who=Str(bo)");
}

#[test]
fn t_without_a_key_is_a_render_error_not_a_panic() {
    let s = Scratch::new("t-nokey").with_default_theme();
    s.write("themes/default/templates/p.html", "{{ t() }}");

    let tera = s.build().unwrap();
    assert!(tera
        .render("default/templates/p.html", &Context::new())
        .is_err());
}

/// Resolves format keys the way a real catalog would, so the date/number
/// filters can be tested without shipping fixtures.
struct FormatTranslator;

#[async_trait::async_trait]
impl Translator for FormatTranslator {
    fn translate(&self, _: &Locale, key: &str, args: &[(&str, TransArg)]) -> String {
        let get = |name: &str| {
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| match v {
                    TransArg::Str(s) => s.clone(),
                    TransArg::Int(i) => i.to_string(),
                    TransArg::Float(f) => f.to_string(),
                })
                .unwrap_or_default()
        };
        match key {
            "format-date" => format!("{} {}, {}", get("month"), get("day"), get("year")),
            "format-datetime" => format!(
                "{} {}, {} {}:{}",
                get("month"),
                get("day"),
                get("year"),
                get("hour"),
                get("minute")
            ),
            "format-thousands-separator" => ".".to_string(),
            k if k.starts_with("month-short-") => {
                format!("M{}", k.trim_start_matches("month-short-"))
            }
            other => other.to_string(),
        }
    }
    fn has_key(&self, _: &Locale, _: &str) -> bool {
        true
    }
    fn available_locales(&self) -> Vec<Locale> {
        vec![Locale::default_locale()]
    }
    fn default_locale_keys(&self) -> Vec<String> {
        Vec::new()
    }
    fn js_strings(
        &self,
        _locale: &ferum_domain::Locale,
    ) -> std::sync::Arc<std::collections::BTreeMap<String, String>> {
        // These stubs exist to drive template rendering, not i18n; the `js-`
        // dictionary only reaches the <meta> tag, which no test here reads.
        Default::default()
    }
    async fn reload(&self) -> std::result::Result<(), ferum_application::shared::AppError> {
        Ok(())
    }
}

fn render_with(translator: Arc<dyn Translator>, body: &str) -> String {
    let s = Scratch::new("fmt").with_default_theme();
    s.write("themes/default/templates/p.html", body);
    let tera = TeraEngine::build_tera(
        &s.0.join("themes"),
        &s.0.join("templates"),
        &s.0.join("static"),
        &Locale::default_locale(),
        &translator,
    )
    .unwrap();
    tera.render("default/templates/p.html", &Context::new())
        .unwrap()
}

#[test]
fn localdate_uses_catalog_month_names_and_field_order() {
    // The whole reason `date(format="%b %d, %Y")` had to go: chrono's month
    // names are always English and the field order is fixed in the pattern.
    let out = render_with(
        Arc::new(FormatTranslator),
        r#"{% set ts = "2026-03-09T14:05:00+00:00" %}{{ ts | localdate }}"#,
    );
    assert_eq!(out, "M3 9, 2026");
}

#[test]
fn localdate_datetime_style_zero_pads_time() {
    // Fluent would render a bare number as "5", never "05", so the filter
    // pads before handing the values over.
    let out = render_with(
        Arc::new(FormatTranslator),
        r#"{% set ts = "2026-03-09T04:05:00+00:00" %}{{ ts | localdate(style="datetime") }}"#,
    );
    assert_eq!(out, "M3 9, 2026 04:05");
}

#[test]
fn localdate_passes_through_unparseable_input() {
    // A malformed timestamp must not take down the page it appears on.
    let out = render_with(
        Arc::new(FormatTranslator),
        r#"{{ "not-a-date" | localdate }}"#,
    );
    assert_eq!(out, "not-a-date");
}

#[test]
fn thousands_separator_comes_from_the_catalog() {
    let out = render_with(Arc::new(FormatTranslator), "{{ 15000000 | thousands }}");
    assert_eq!(out, "15.000.000");
}

#[test]
fn thousands_falls_back_to_comma_when_catalog_lacks_the_key() {
    // StubTranslator returns the key itself, which is far longer than one
    // character — the filter must not splice that in as a separator.
    let out = render_with(Arc::new(StubTranslator), "{{ 15000000 | thousands }}");
    assert_eq!(out, "15,000,000");
}

#[test]
fn translated_output_is_html_escaped() {
    // Catalogs are admin-editable, so a translation must never be trusted as
    // markup. If this ever fails, `| safe` has crept in somewhere.
    struct HostileTranslator;
    #[async_trait::async_trait]
    impl Translator for HostileTranslator {
        fn translate(&self, _: &Locale, _: &str, _: &[(&str, TransArg)]) -> String {
            "<script>alert(1)</script>".to_string()
        }
        fn has_key(&self, _: &Locale, _: &str) -> bool {
            true
        }
        fn available_locales(&self) -> Vec<Locale> {
            vec![Locale::default_locale()]
        }
        fn default_locale_keys(&self) -> Vec<String> {
            Vec::new()
        }
        fn js_strings(
            &self,
            _locale: &ferum_domain::Locale,
        ) -> std::sync::Arc<std::collections::BTreeMap<String, String>> {
            // These stubs exist to drive template rendering, not i18n; the `js-`
            // dictionary only reaches the <meta> tag, which no test here reads.
            Default::default()
        }
        async fn reload(&self) -> std::result::Result<(), ferum_application::shared::AppError> {
            Ok(())
        }
    }

    let s = Scratch::new("t-escape").with_default_theme();
    s.write("themes/default/templates/p.html", r#"{{ t(k="x") }}"#);

    let translator: Arc<dyn Translator> = Arc::new(HostileTranslator);
    let tera = TeraEngine::build_tera(
        &s.0.join("themes"),
        &s.0.join("templates"),
        &s.0.join("static"),
        &Locale::default_locale(),
        &translator,
    )
    .unwrap();

    let out = tera
        .render("default/templates/p.html", &Context::new())
        .unwrap();
    assert!(!out.contains("<script>"), "translation was not escaped: {out}");
}
