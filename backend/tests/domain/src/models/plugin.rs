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
