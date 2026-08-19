//! Per-locale copy for the handful of admin-editable `site_config` values that a
//! visitor reads.
//!
//! Storage is the existing flat key space, suffixed: `site_tagline` holds the copy
//! for `Locale::DEFAULT_TAG` and `site_tagline:vi` a translation. A suffixed key is
//! written only when an admin types a translation, so a single-language site has
//! none and resolves exactly as it did before this module existed.
//!
//! **Keyed on the SOURCE locale, never the site default.** `site_config.default_locale`
//! is configurable, and keying the bare row on it would mean changing that setting
//! silently reassigns which language the bare row holds — orphaning a `:vi` row the
//! moment `vi` became the site default. The bare row is `Locale::DEFAULT_TAG`'s copy
//! and the last-resort fallback for every language, and that never moves.
//!
//! `site_config` rather than a table of its own because [`resolve`] runs on every
//! page render, against the whole-table `HashMap` already held in memory. A table
//! would mean either a second cache with its own invalidation or a query on the
//! render path.

use std::collections::HashMap;

use crate::Locale;

/// Separates a base key from its locale tag.
///
/// `:` is safe as a delimiter because `Locale::parse` rejects it, so it cannot occur
/// inside the tag half and `split_once` can never pick the wrong point.
const LOCALE_KEY_SEPARATOR: char = ':';

/// The `site_config` keys that carry per-locale copy.
///
/// Adding one here is the whole change on the storage side: it becomes writable
/// through `/api/admin/config` and gains a field per installed locale on the
/// settings page.
pub const LOCALIZED_CONFIG_KEYS: &[&str] = &["site_tagline", "site_slogan"];

/// The storage key holding `base`'s copy in `locale`.
///
/// `Locale::DEFAULT_TAG` keeps the bare key: it is what the setup wizard writes,
/// what every existing install already has, and the last-resort fallback for a
/// language with no translation of its own.
pub fn localized_key(base: &str, locale: &Locale) -> String {
    if locale.is_source_locale() {
        return base.to_string();
    }
    format!("{base}{LOCALE_KEY_SEPARATOR}{locale}")
}

/// True when `key` is a per-locale form of a [`LOCALIZED_CONFIG_KEYS`] entry.
///
/// The tag half must be a **canonical** `Locale`, so `site_tagline:EN-us` is
/// rejected rather than accepted as a third spelling of `en-US` — two spellings
/// would be two rows, only one of which is ever read, making the other an edit that
/// appears to save and does nothing.
///
/// The base half is checked too. Without it a locale suffix would be a bypass for
/// the writable-key allowlist, `smtp_pass:vi` included.
pub fn is_writable_localized_key(key: &str) -> bool {
    let Some((base, tag)) = key.split_once(LOCALE_KEY_SEPARATOR) else {
        // A bare key is the static allowlist's business, not this one's.
        return false;
    };
    let Some(locale) = Locale::parse(tag) else {
        return false;
    };
    // `Locale::DEFAULT_TAG` never appears as a suffix — its copy is the bare key —
    // so accepting one here would create a row nothing reads.
    locale.as_str() == tag && !locale.is_source_locale() && LOCALIZED_CONFIG_KEYS.contains(&base)
}

/// `base`'s copy in `locale`, resolved over the locale's fallback chain, or the
/// empty string when nothing is set.
///
/// The bare key needs no separate lookup: `fallback_chain` always ends at
/// `Locale::DEFAULT_TAG`, and [`localized_key`] maps that to the bare key, so it is
/// the chain's last link by construction.
///
/// Empty is treated as absent throughout: an admin who clears a field posts `""`
/// and `update_config` stores it, so a blank translation must fall through rather
/// than blank out a page the source-locale copy could have filled.
pub fn resolve(config: &HashMap<String, String>, base: &str, locale: &Locale) -> String {
    locale
        .fallback_chain()
        .into_iter()
        .find_map(|link| {
            config
                .get(&localized_key(base, &link))
                .filter(|v| !v.is_empty())
        })
        .cloned()
        .unwrap_or_default()
}
