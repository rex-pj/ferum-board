//! Guards the templates against silent damage.
//!
//! These exist because string extraction across ~1,400 sites was done by a
//! rewriting script, and a script that edits HTML can corrupt it in ways a
//! parse test cannot see: Tera happily parses a template whose `<script>` tag
//! has been swallowed, and every existing test passed while `base.html` was
//! missing `ferum-api.js`.
//!
//! What each test asserts is therefore a *structural invariant*, not a string.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn frontend() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("frontend")
}

fn html_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Vendored packages are not ours to police.
            if path.file_name().is_some_and(|n| n == "node_modules") {
                continue;
            }
            html_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "html") {
            out.push(path);
        }
    }
}

fn all_templates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    html_files(&frontend().join("themes"), &mut out);
    html_files(&frontend().join("templates"), &mut out);
    out
}

/// Strips Tera constructs so only real markup is left.
fn markup_only(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &src[i..];
        let skip_to = |open: &str, close: &str| -> Option<usize> {
            rest.starts_with(open).then(|| {
                rest.find(close)
                    .map(|e| e + close.len())
                    .unwrap_or(rest.len())
            })
        };
        if let Some(n) = skip_to("{#", "#}")
            .or_else(|| skip_to("{{", "}}"))
            .or_else(|| skip_to("{%", "%}"))
        {
            i += n;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[test]
fn no_template_lost_its_script_or_link_tags() {
    // The concrete failure this catches: a `{# … #}` comment that mentions
    // `<script>` in prose, masked *after* a `<script>…</script>` guard, causes
    // the guard to match from inside the comment to the next real closing tag —
    // deleting the real tag. `base.html` lost `ferum-api.js` exactly this way.
    for file in all_templates() {
        let src = std::fs::read_to_string(&file).unwrap();
        let markup = markup_only(&src);

        let opens = markup.matches("<script").count();
        let closes = markup.matches("</script>").count();
        assert_eq!(
            opens,
            closes,
            "unbalanced <script> tags in {} — {opens} open, {closes} closed. \
             A rewriting script has probably eaten one.",
            file.display()
        );
    }
}

#[test]
fn base_templates_still_load_their_required_scripts() {
    // Named explicitly rather than counted: these are the ones whose absence
    // breaks the page silently at runtime instead of failing a build.
    let checks: &[(&str, &[&str])] = &[
        (
            "themes/default/templates/base.html",
            &["ferum-api.js", "ferum-preload.js", "ferum-utils.js"],
        ),
        (
            "templates/admin/base.html",
            &["ferum-api.js", "ferum-utils.js", "ferum-admin.js"],
        ),
    ];

    for (rel, required) in checks {
        let path = frontend().join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for script in *required {
            assert!(
                src.contains(script),
                "{rel} no longer loads {script} — pages depending on it will fail at runtime"
            );
        }
    }
}

#[test]
fn no_template_contains_extraction_sentinels_or_stray_markers() {
    // The rewriter masks regions with control characters and restores them
    // afterwards. A leftover sentinel means a restore failed and content is gone.
    for file in all_templates() {
        let src = std::fs::read_to_string(&file).unwrap();
        for (label, ch) in [("\\u0001", '\u{1}'), ("\\u0002", '\u{2}')] {
            assert!(
                !src.contains(ch),
                "{} contains a leftover extraction sentinel ({label}) — a masked \
                 region was never restored",
                file.display()
            );
        }
    }
}

#[test]
fn translated_strings_are_never_marked_safe() {
    // Catalogs become admin-editable in Step 7 Tier 3. Piping a translation
    // through `| safe` would turn that into stored XSS on every page that
    // renders it.
    for file in all_templates() {
        let src = std::fs::read_to_string(&file).unwrap();
        for line in src.lines() {
            if line.contains("t(k=") && line.contains("| safe") {
                panic!(
                    "{} pipes a translated string through `| safe`:\n  {}",
                    file.display(),
                    line.trim()
                );
            }
        }
    }
}

#[test]
fn no_template_still_uses_the_locale_blind_date_filter() {
    // `date(format="%b …")` renders English month names regardless of locale.
    // `localdate` takes both the month name and the field order from the catalog.
    let mut offenders = BTreeSet::new();
    for file in all_templates() {
        let src = std::fs::read_to_string(&file).unwrap();
        if src.contains("date(format=") {
            offenders.insert(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these templates use `date(format=…)`, which cannot be localized — \
         use `| localdate` instead:\n  {}",
        offenders.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}

/// Renders a template with the real catalogs and a minimal context.
async fn render(template: &str, ctx: tera::Context) -> String {
    let root = frontend();
    let locales = root.parent().unwrap().join("locales");
    let translator: std::sync::Arc<dyn ferum_application::ports::Translator> =
        std::sync::Arc::new(
            ferum_infrastructure::i18n::FluentTranslator::new(vec![locales]).await,
        );
    let engine = ferum_web::tera_engine::TeraEngine::new(
        root.join("themes"),
        root.join("templates"),
        root.join("static"),
        translator,
    )
    .expect("engine builds");
    engine
        .render(&ferum_domain::Locale::parse("vi").unwrap(), template, ctx)
        .await
        .unwrap_or_else(|e| panic!("render {template} failed: {e}"))
}

fn ctx_with_two_locales() -> tera::Context {
    let mut ctx = tera::Context::new();
    ctx.insert("available_locales", &vec!["en", "vi"]);
    ctx.insert("locale", "vi");
    ctx.insert("site", &serde_json::json!({ "name": "Ferum" }));
    ctx.insert("default_theme_slug", "default");
    // Both shells include `notif_bell.html`, which dereferences `current_user.id`
    // unguarded. Signed-in is also the state that actually exercises the header:
    // the theme toggle and the switcher sit next to the bell and the avatar.
    ctx.insert(
        "current_user",
        &serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "username": "tester",
            "display_name": "Tester",
            "unread_count": 0,
            "is_admin": true,
            "is_moderator": true,
        }),
    );
    ctx
}

#[tokio::test]
async fn language_switcher_appears_in_both_shells_left_of_the_theme_toggle() {
    // The switcher must exist in the admin/mod shell too, not just the public
    // one — the panels were the only place a user's chosen language had no
    // control. Position is asserted as well as presence: it belongs to the left
    // of the theme toggle in both, and "consistent" is the whole point.
    for shell in [
        "default/templates/partials/nav.html",
        "admin/base.html",
    ] {
        let html = render(shell, ctx_with_two_locales()).await;

        let switcher = html.find("data-ferum-locale").unwrap_or_else(|| {
            panic!("{shell} has no language switcher")
        });
        let toggle = html.find("data-ferum-theme-btn").unwrap_or_else(|| {
            panic!("{shell} has no theme toggle")
        });

        assert!(
            switcher < toggle,
            "{shell}: language switcher must come before the theme toggle \
             (switcher at {switcher}, toggle at {toggle})"
        );
    }
}

#[tokio::test]
async fn language_switcher_is_absent_when_only_one_locale_is_installed() {
    // A control that cannot change anything is worse than no control.
    let mut ctx = ctx_with_two_locales();
    ctx.insert("available_locales", &vec!["en"]);

    for shell in ["default/templates/partials/nav.html", "admin/base.html"] {
        let html = render(shell, ctx.clone()).await;
        assert!(
            !html.contains("data-ferum-locale"),
            "{shell} renders a language switcher with only one locale installed"
        );
    }
}

#[tokio::test]
async fn both_shells_use_the_same_brand_mark() {
    // Admin used `.adm-brand-logo` while the forum used `.fr-brand-mark`, so the
    // badge changed size and colour when crossing into the panels.
    for shell in ["default/templates/partials/nav.html", "admin/base.html"] {
        let html = render(shell, ctx_with_two_locales()).await;
        assert!(
            html.contains("fr-brand-mark"),
            "{shell} does not use the shared `fr-brand-mark` badge"
        );
    }
}

// ─── Timestamps must be client-correctable ────────────────────────────────────

/// Every `| localdate` must sit inside a `<time>` carrying `data-rel` or
/// `data-abs`.
///
/// The filter renders **UTC**, and deliberately so — it localises month names
/// and field order through the Fluent catalog, which is all the server can do
/// without knowing the reader's zone. Turning that into the reader's actual
/// local time is `ferum-utils.js`'s job, and it only looks at `<time>` elements
/// carrying one of those two markers.
///
/// So a bare `{{ ts | localdate }}` is not "unstyled" — it is a timestamp
/// permanently stuck in UTC, which for a reader at +07 shows the previous day's
/// date for anything posted before 07:00 local. There were six of them, and
/// every one looked completely fine on a UTC developer machine.
#[test]
fn every_localdate_is_inside_a_client_corrected_time_element() {
    let mut offenders = Vec::new();

    for path in all_templates() {
        let Ok(src) = std::fs::read_to_string(&path) else { continue };

        for (idx, _) in src.match_indices("| localdate").chain(src.match_indices("|localdate")) {
            let before = &src[..idx];

            // Are we inside a <time> element? The nearest unclosed `<time` wins.
            let open = before.rfind("<time");
            let close = before.rfind("</time>");
            let inside = match (open, close) {
                (Some(o), Some(c)) => o > c,
                (Some(_), None) => true,
                _ => false,
            };

            let line = before.matches('\n').count() + 1;

            if !inside {
                offenders.push(format!(
                    "{}:{line} — `localdate` outside any <time> element",
                    path.display()
                ));
                continue;
            }

            // It is inside one; now check the opening tag carries a marker.
            let tag_start = open.expect("inside implies an opening tag");
            let tag = &src[tag_start..];
            let tag_end = tag.find('>').map(|e| tag_start + e).unwrap_or(src.len());
            let opening = &src[tag_start..tag_end];

            if !opening.contains("data-rel") && !opening.contains("data-abs") {
                offenders.push(format!(
                    "{}:{line} — <time> has neither data-rel nor data-abs, so \
                     ferum-utils.js will never rewrite it and the value stays UTC",
                    path.display()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "\n\n{} server-rendered timestamp(s) the client cannot correct:\n\n{}\n\n\
         Wrap the value in <time data-abs data-style=\"date|datetime|daymonth|monthyear\" \
         datetime=\"{{{{ ts }}}}\"> for an absolute date, or <time data-rel datetime=…> \
         for a relative one.\n",
        offenders.len(),
        offenders.join("\n")
    );
}

/// A timestamp must never be rendered by slicing its ISO string.
///
/// `{{ ts | truncate(length=10) }}` yields the first ten characters of
/// "2026-08-08T23:30:00Z" — the UTC calendar date, with the zone information
/// thrown away and the catalog's date formatting bypassed. Two templates did
/// this, and the result is a moderator queue dated a day behind for anyone east
/// of UTC.
#[test]
fn no_template_renders_a_date_by_truncating_an_iso_string() {
    let mut offenders = Vec::new();

    for path in all_templates() {
        let Ok(src) = std::fs::read_to_string(&path) else { continue };
        for (idx, _) in src.match_indices("truncate(length=10") {
            let line = src[..idx].matches('\n').count() + 1;
            offenders.push(format!("{}:{line}", path.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "\n\nISO-string slicing used as a date format in {} place(s):\n{}\n\n\
         Use `| localdate` inside a <time data-abs> instead.\n",
        offenders.len(),
        offenders.join("\n")
    );
}
