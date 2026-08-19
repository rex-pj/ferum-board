//! Conventions the browser bundle has to keep, checked by scanning the tree.
//!
//! Nothing compiles these files, so a drift here has no other alarm — the same
//! reason `template_keys.rs` scans templates for unresolvable `t()` keys.

use std::fs;
use std::path::PathBuf;

fn js_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("frontend")
        .join("static")
        .join("js")
}

/// Our own scripts, excluding vendored bundles we do not author.
fn our_scripts() -> Vec<(String, String)> {
    fs::read_dir(js_dir())
        .expect("frontend/static/js must exist")
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !name.ends_with(".js") || !name.starts_with("ferum-") {
                return None;
            }
            // Compiled output, not a source file we edit.
            if name.contains(".iife.") {
                return None;
            }
            Some((name, fs::read_to_string(&path).ok()?))
        })
        .collect()
}

/// A bare `confirm(` — `window.confirm(x)` or `confirm(x)` — but not
/// `Ferum.showConfirm(` (capital C) or an identifier like `confirmThen(`.
fn calls_native_confirm(src: &str) -> bool {
    src.match_indices("confirm").any(|(at, _)| {
        let before_is_word = at > 0
            && src[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if before_is_word {
            return false;
        }
        src[at + "confirm".len()..]
            .trim_start()
            .starts_with('(')
    })
}

/// **One confirmation dialog, everywhere.**
///
/// `Ferum.showConfirm` is a Bootstrap modal that matches the rest of the UI,
/// can be styled and translated, and supports type-to-confirm. The browser's
/// own dialog matches nothing, cannot be translated, and is suppressible by the
/// user — which for a delete confirmation means the delete just happens.
///
/// The exception a future author will reach for: a *cancellable* event handler
/// such as `hide.bs.modal`, where the native dialog's synchrony is genuinely
/// load-bearing. `ferum-admin-products.js` had exactly that case; it is solved
/// by cancelling the close and re-issuing it once the promise resolves, not by
/// reaching back for `confirm`.
#[test]
fn no_script_uses_the_browsers_own_confirm_dialog() {
    let offenders: Vec<String> = our_scripts()
        .into_iter()
        .filter(|(_, src)| calls_native_confirm(src))
        .map(|(name, _)| name)
        .collect();

    assert!(
        offenders.is_empty(),
        "these use window.confirm instead of Ferum.showConfirm: {offenders:?}"
    );
}

/// The helper only earns its keep if it is actually reachable, and it lives in
/// the one file every page already loads.
#[test]
fn the_shared_dialog_is_exported_from_ferum_utils() {
    let src = fs::read_to_string(js_dir().join("ferum-utils.js")).expect("ferum-utils.js");
    assert!(
        src.contains("showConfirm:"),
        "Ferum.showConfirm must stay exported — every admin surface calls it"
    );
}

/// It is reachable from public pages, so its own chrome cannot be English
/// literals. The OK label is overwritten per call; Cancel and the close button
/// never are, which makes them the ones that go stale unnoticed.
#[test]
fn the_shared_dialog_translates_its_own_buttons() {
    let src = fs::read_to_string(js_dir().join("ferum-utils.js")).expect("ferum-utils.js");
    let modal = src
        .split("function ensureConfirmModal")
        .nth(1)
        .expect("ensureConfirmModal must exist");
    let modal = &modal[..modal.find("\n  function ").unwrap_or(modal.len())];

    assert!(
        !modal.contains(">Cancel<") && !modal.contains("aria-label=\"Close\""),
        "the dialog's own buttons must go through Ferum.t, not English literals"
    );
    for key in ["js-cancel", "js-close"] {
        assert!(modal.contains(key), "{key} must be used by the dialog markup");
    }
}
