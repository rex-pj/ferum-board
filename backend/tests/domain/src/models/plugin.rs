//! The UI-slot custom element naming rule — a contract between the server, which
//! emits the tag, and a plugin bundle, which calls `customElements.define`.
//!
//! Nothing checks they agree at build time and a mismatch raises no error: an
//! unknown custom element is a valid empty box, so the widget just never
//! appears. The slug is in the name because omitting it made two plugins in one
//! slot race for the same tag.

use ferum_domain::models::plugin::ui_slot_element_tag;

/// The four bundles under `examples/plugins/` hardcode these strings. Changing
/// the rule without updating them is the silent-failure case above, so the exact
/// expected output is written out here rather than recomputed.
#[test]
fn shipped_example_plugins_get_the_tags_their_bundles_define() {
    for (slug, slot, expected) in [
        (
            "com.ferum.home-hero",
            "home_feed_top",
            "ferum-slot-com-ferum-home-hero-home-feed-top",
        ),
        (
            "com.ferum.simple-chatbox",
            "home_feed_top",
            "ferum-slot-com-ferum-simple-chatbox-home-feed-top",
        ),
        (
            "com.ferum.announcement-banner",
            "content_before",
            "ferum-slot-com-ferum-announcement-banner-content-before",
        ),
        (
            "com.ferum.community-polls",
            "sidebar_left_top",
            "ferum-slot-com-ferum-community-polls-sidebar-left-top",
        ),
    ] {
        assert_eq!(
            ui_slot_element_tag(slug, slot),
            expected,
            "tag for {slug} in {slot} changed — examples/plugins/*/bundle.js must be \
             updated in the same commit, or the widget silently stops rendering"
        );
    }
}

/// The whole reason the slug is in the name.
#[test]
fn two_plugins_in_one_slot_get_distinct_tags() {
    let hero = ui_slot_element_tag("com.ferum.home-hero", "home_feed_top");
    let chat = ui_slot_element_tag("com.ferum.simple-chatbox", "home_feed_top");

    assert_ne!(
        hero, chat,
        "plugins sharing a slot must own separate elements — identical tags make \
         one plugin render in both slot positions and the other in neither"
    );
}

/// A slot name is not the only input, so the same plugin in two slots must not
/// collapse to one name either.
#[test]
fn one_plugin_in_two_slots_gets_distinct_tags() {
    assert_ne!(
        ui_slot_element_tag("com.example.p", "content_before"),
        ui_slot_element_tag("com.example.p", "content_after"),
    );
}

/// Slugs come from a manifest an admin uploaded. The output has to be a *valid*
/// custom element name whatever is in there: lowercase, at least one hyphen, and
/// starting with an ASCII letter — a name starting with a digit is rejected by
/// `customElements.define` with a `SyntaxError` that takes the whole bundle down,
/// not just the one widget.
#[test]
fn hostile_slugs_still_produce_a_valid_element_name() {
    for slug in [
        "9lives",
        "UPPER.Case_Plugin",
        "sp ace",
        "trailing---dashes---",
        "sym+bols!@#",
        "..dots..",
    ] {
        let tag = ui_slot_element_tag(slug, "content_before");

        assert!(
            tag.starts_with("ferum-slot-"),
            "{slug}: the constant prefix is what guarantees a leading letter"
        );
        assert!(tag.contains('-'), "{slug}: a custom element name requires a hyphen");
        assert_eq!(tag, tag.to_lowercase(), "{slug}: must be lowercase");
        assert!(
            tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{slug}: produced `{tag}`, which contains a character invalid in an element name"
        );
        assert!(
            !tag.contains("--"),
            "{slug}: produced `{tag}` — runs of separators must collapse, or two \
             different slugs can differ only by dash count"
        );
        assert!(!tag.ends_with('-'), "{slug}: produced a trailing separator");
    }
}

/// Two slugs that differ only in characters the sanitizer folds would otherwise
/// map to one element, which is the collision the slug was added to prevent.
/// This documents the residual case rather than claiming it cannot happen:
/// `a.b` and `a-b` DO collide, and the slug uniqueness constraint on `plugins`
/// is what keeps that theoretical.
#[test]
fn folding_is_documented_where_it_is_lossy() {
    assert_eq!(
        ui_slot_element_tag("a.b", "s"),
        ui_slot_element_tag("a-b", "s"),
        "separator folding is lossy by design; two slugs differing only in \
         separator characters cannot both be installed anyway"
    );
    assert_ne!(
        ui_slot_element_tag("a.b", "s"),
        ui_slot_element_tag("ab", "s"),
        "but a separator must not vanish entirely — that would collide slugs \
         that are genuinely different"
    );
}

// ─── Which config fields are credentials ────────────────────────────────────
//
// `secret_config_keys` decides what the plugin repository encrypts at rest. Two
// failure directions, and they are not symmetric: missing a key leaves a
// credential in plaintext in every backup, while inventing one seals a value the
// admin expects to read back and edit. Both are silent.

use ferum_domain::models::plugin::secret_config_keys;

#[test]
fn reads_the_secret_flag_from_config_schema() {
    let manifest = serde_json::json!({
        "config_schema": {
            "properties": {
                "webhook_url":    { "type": "string",  "secret": true },
                "notify_on_post": { "type": "boolean" },
                "username":       { "type": "string",  "secret": false },
            }
        }
    });
    assert_eq!(secret_config_keys(&manifest), vec!["webhook_url"]);
}

#[test]
fn only_a_literal_true_marks_a_field_secret() {
    // A truthy-looking value is not a flag. TOML has real booleans, so `secret =
    // "true"` is an author error — and silently honouring it would mean the
    // opposite mistake (`secret = "false"`) also seals, which is worse.
    for spec in [
        serde_json::json!({ "secret": "true" }),
        serde_json::json!({ "secret": 1 }),
        serde_json::json!({ "secret": "yes" }),
    ] {
        let manifest = serde_json::json!({ "config_schema": { "properties": { "k": spec } } });
        assert!(
            secret_config_keys(&manifest).is_empty(),
            "only a boolean true may mark a field secret"
        );
    }
}

#[test]
fn a_manifest_without_a_config_schema_yields_nothing() {
    // The common case — most plugins take no configuration at all. Returning
    // empty rather than erroring is what keeps this safe to call on every read.
    for manifest in [
        serde_json::json!({}),
        serde_json::json!({ "config_schema": {} }),
        serde_json::json!({ "config_schema": { "properties": {} } }),
        // Malformed shapes must not make an installed plugin unreadable.
        serde_json::json!({ "config_schema": "nonsense" }),
        serde_json::json!({ "config_schema": { "properties": ["a", "b"] } }),
        serde_json::json!({ "config_schema": { "properties": { "k": "not-an-object" } } }),
    ] {
        assert!(secret_config_keys(&manifest).is_empty(), "{manifest}");
    }
}

#[test]
fn the_result_is_sorted_so_it_does_not_depend_on_map_order() {
    let manifest = serde_json::json!({
        "config_schema": {
            "properties": {
                "zeta":  { "secret": true },
                "alpha": { "secret": true },
                "mid":   { "secret": true },
            }
        }
    });
    assert_eq!(secret_config_keys(&manifest), vec!["alpha", "mid", "zeta"]);
}

#[test]
fn the_shipped_discord_notifier_marks_its_webhook_url_secret() {
    // The one credential among the example plugins. A Discord webhook URL needs
    // no further authentication — holding it is enough to post to the channel —
    // so it must not sit in plaintext in `plugins.config`.
    let path = examples_plugins_dir()
        .join("discord-notifier")
        .join("plugin.toml");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    let webhook_section = src
        .split("[config_schema.properties.")
        .find(|s| s.starts_with("webhook_url]"))
        .expect("discord-notifier declares a webhook_url config field");
    // Stop at the next section so a `secret = true` further down cannot satisfy
    // this by accident.
    let body = webhook_section.split("\n[").next().unwrap();

    assert!(
        body.lines()
            .any(|l| l.split('#').next().unwrap().trim() == "secret      = true"
                || l.split('#').next().unwrap().replace(' ', "") == "secret=true"),
        "discord-notifier's webhook_url must carry `secret = true`; without it the \
         credential is stored in plaintext even when SECRET_ENCRYPTION_KEY is set"
    );
}

// ─── Drift guard against the shipped bundles ────────────────────────────────
//
// The assertions above pin the *rule*. They cannot catch the other half of the
// contract: a bundle that was never updated to match it. That half is where the
// silence lives — the server emits the new tag, the bundle defines the old one,
// and the page renders an empty element with nothing logged anywhere.
//
// Reading the files also means a plugin added later is covered without anyone
// remembering to extend a hardcoded list.

use std::path::{Path, PathBuf};

fn examples_plugins_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("examples")
        .join("plugins")
}

/// Minimal reader for the two things needed here: `id` under `[meta]`, and the
/// slot names declared as `[ui_slots.<name>]` tables.
///
/// Deliberately not a TOML parser. Pulling `toml` into this crate to read two
/// keys would mean the pure-domain test harness — which exists partly to prove
/// `ferum-domain` builds against nothing — grows a dependency, and the manifests
/// here are flat enough that a line scan is honest.
fn read_manifest(path: &Path) -> Option<(String, Vec<String>)> {
    let src = std::fs::read_to_string(path).ok()?;
    let mut id = None;
    let mut slots = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if id.is_none() {
            if let Some(rest) = line.strip_prefix("id") {
                if let Some(value) = rest.trim_start().strip_prefix('=') {
                    id = Some(value.trim().trim_matches('"').to_string());
                }
            }
        }
        if let Some(rest) = line.strip_prefix("[ui_slots.") {
            if let Some(name) = rest.strip_suffix(']') {
                slots.push(name.to_string());
            }
        }
    }

    id.map(|id| (id, slots))
}

#[test]
fn every_example_bundle_defines_the_tag_the_server_will_emit() {
    let dir = examples_plugins_dir();
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));

    let mut checked = 0;

    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }

        let Some((slug, slots)) = read_manifest(&plugin_dir.join("plugin.toml")) else {
            continue;
        };
        if slots.is_empty() {
            continue; // Manifest-tier plugin, or one that injects no UI.
        }

        let bundle_path = plugin_dir.join("bundle.js");
        let bundle = std::fs::read_to_string(&bundle_path).unwrap_or_else(|e| {
            panic!(
                "{} declares {} UI slot(s) but its bundle could not be read: {e}",
                plugin_dir.display(),
                slots.len()
            )
        });

        for slot in slots {
            let tag = ui_slot_element_tag(&slug, &slot);
            assert!(
                bundle.contains(&tag),
                "{} declares [ui_slots.{slot}], so the server will emit <{tag}>, \
                 but {} never mentions that name. The page would render an empty \
                 element with no error anywhere.",
                plugin_dir.display(),
                bundle_path.display(),
            );
            checked += 1;
        }
    }

    // Finding nothing must never read as success: a moved or renamed examples/
    // directory would otherwise make this pass having compared nothing.
    assert!(
        checked > 0,
        "no example plugin with a UI slot was found under {} — has the directory moved?",
        dir.display()
    );
}
