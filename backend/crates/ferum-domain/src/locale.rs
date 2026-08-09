//! Locale value type.
//!
//! A `Locale` is a *shape-validated* BCP-47 language tag. Construction is
//! fallible, which is what lets the rest of the system treat a locale as safe
//! to interpolate into a filesystem path (`locales/{locale}/errors.ftl`) or a
//! URL prefix without re-checking it. An attacker-controlled path segment can
//! never reach a bundle lookup, because `parse` rejects anything containing a
//! separator, a dot, or a non-ASCII byte.
//!
//! Shape validity is deliberately *not* the same thing as being enabled on this
//! site. `Locale::parse("ja")` succeeds even on a site that only ships English —
//! whether a locale is actually available is runtime configuration owned by the
//! admin (`site_config.enabled_locales`), resolved above the domain layer. Keeping
//! the roster out of the type is what allows a language pack to be uploaded at
//! runtime without a recompile, if that is ever built (see docs/i18n.md).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// Longest tag we accept, e.g. `zh-Hant-TW` is 10 bytes. The cap exists so a
/// hostile tag can never be used to blow up a path buffer or a cache key.
const MAX_TAG_LEN: usize = 12;

/// A shape-validated BCP-47 language tag, normalized to a canonical casing.
///
/// Cloning is cheap in practice — tags are 2–10 bytes — and the owned `String`
/// (rather than `&'static str`) is what permits locales that were not known at
/// compile time, such as an uploaded language pack.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize)]
pub struct Locale(String);

impl Locale {
    /// The fallback every other locale ultimately resolves through. Also the
    /// locale whose catalog defines the canonical key set used for coverage
    /// reporting in the admin UI.
    pub const DEFAULT_TAG: &'static str = "en";

    pub fn default_locale() -> Locale {
        Locale(Self::DEFAULT_TAG.to_string())
    }

    /// Parses and canonicalizes a language tag, returning `None` if the shape is
    /// not a plausible BCP-47 tag.
    ///
    /// Accepted: `en`, `vi`, `en-US`, `zh-Hant`, `zh-Hant-TW`.
    /// Rejected: anything with `/`, `\`, `.`, `..`, whitespace, non-ASCII, an
    /// empty subtag, or more than three subtags.
    ///
    /// Input casing is irrelevant — `EN-us` and `en-US` both canonicalize to
    /// `en-US` — so a cookie or URL cannot produce two `Locale`s that name the
    /// same language yet compare unequal and split the catalog cache.
    pub fn parse(tag: &str) -> Option<Locale> {
        if tag.is_empty() || tag.len() > MAX_TAG_LEN || !tag.is_ascii() {
            return None;
        }

        let mut parts = tag.split('-');

        // Subtag 1 — language: 2 or 3 alphabetic characters, lowercased.
        let language = parts.next()?;
        if !matches!(language.len(), 2 | 3) || !language.bytes().all(|b| b.is_ascii_alphabetic()) {
            return None;
        }
        let mut out = language.to_ascii_lowercase();

        // Subtag 2 — script (4 alpha, Titlecase) or region (2 alpha / 3 digit).
        // BCP-47 orders script before region, so a 4-letter subtag can only be a
        // script and must be followed by at most a region.
        if let Some(second) = parts.next() {
            out.push('-');
            out.push_str(&canonical_subtag(second)?);

            if let Some(third) = parts.next() {
                // Only script-then-region is valid here; a second region is not.
                if second.len() != 4 {
                    return None;
                }
                out.push('-');
                out.push_str(&canonical_region(third)?);
            }
        }

        // More than three subtags — variants, extensions, private use — are not
        // something this system has a use for, and each one is another shape to
        // validate. Reject rather than silently accept and mishandle.
        if parts.next().is_some() {
            return None;
        }

        Some(Locale(out))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when this is the site-wide default. The default locale is served on
    /// unprefixed URLs, so routing needs to ask this often.
    pub fn is_default(&self) -> bool {
        self.0 == Self::DEFAULT_TAG
    }

    /// This locale narrowed toward its base language, most specific first, and
    /// **without** the default locale appended. `zh-Hant-TW` → `["zh-Hant-TW",
    /// "zh-Hant", "zh"]`.
    ///
    /// This is the chain to use when *matching* — deciding whether a requested
    /// language is one this site speaks. Using [`Self::fallback_chain`] there
    /// would be a bug: it ends at the default, so a request for German would
    /// "match" English and a German speaker would silently be served the default
    /// language as though it were their own.
    pub fn base_chain(&self) -> Vec<Locale> {
        let mut chain = vec![self.clone()];

        // Strip trailing subtags one at a time: zh-Hant-TW → zh-Hant → zh.
        let mut current = self.0.as_str();
        while let Some(idx) = current.rfind('-') {
            current = &current[..idx];
            let candidate = Locale(current.to_string());
            if !chain.contains(&candidate) {
                chain.push(candidate);
            }
        }
        chain
    }

    /// The resolution chain for this locale, most specific first, always ending
    /// at the default. `en-US` → `["en-US", "en"]`; `vi` → `["vi", "en"]`.
    ///
    /// This is the chain to use when *resolving a message*: the same shape as
    /// the theme inheritance chain — try each link in order, take the first hit.
    /// A regional catalog only needs to carry the strings that actually differ
    /// from its base language, and anything untranslated ultimately shows in the
    /// default language rather than as a raw key.
    pub fn fallback_chain(&self) -> Vec<Locale> {
        let mut chain = self.base_chain();
        let default = Locale::default_locale();
        if !chain.contains(&default) {
            chain.push(default);
        }
        chain
    }
}

/// A script subtag (`Hant`) canonicalizes to Titlecase; anything else is treated
/// as a region.
fn canonical_subtag(subtag: &str) -> Option<String> {
    if subtag.len() == 4 && subtag.bytes().all(|b| b.is_ascii_alphabetic()) {
        let mut s = subtag.to_ascii_lowercase();
        s[..1].make_ascii_uppercase();
        return Some(s);
    }
    canonical_region(subtag)
}

/// A region subtag is 2 alphabetic characters (uppercased) or 3 digits.
fn canonical_region(subtag: &str) -> Option<String> {
    match subtag.len() {
        2 if subtag.bytes().all(|b| b.is_ascii_alphabetic()) => Some(subtag.to_ascii_uppercase()),
        3 if subtag.bytes().all(|b| b.is_ascii_digit()) => Some(subtag.to_string()),
        _ => None,
    }
}

impl Default for Locale {
    fn default() -> Self {
        Locale::default_locale()
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Deserialization goes through `parse`, so a `Locale` arriving from JSON — an
/// API request body, a persisted preference row — is validated on the same path
/// as one arriving from a URL. There is no way to construct an unvalidated
/// `Locale` from outside this module.
impl<'de> Deserialize<'de> for Locale {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Locale::parse(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid locale tag: {raw}")))
    }
}
