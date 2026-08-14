//! URL-shape helpers shared by every `StorageService` backend.
//!
//! `public_url` and `key_from_url` are inverses, and the reverse direction
//! carries far more weight than its size suggests: file URLs are denormalised
//! into `users.avatar_url`, `site_config` and the stored HTML of every post, and
//! those strings are never rewritten. `key_from_url` failing to recognise one is
//! not an error — it silently reports "not one of ours", so the reference is
//! never taken or never released, and files are either leaked or garbage
//! collected out from under posts still displaying them. Recognising *too much*
//! fails the same way round: a key that names nothing, so the reference lands on
//! a file that does not exist while the real one goes uncounted.
//!
//! Every rule below is identical in all four backends, which is why it lives
//! here rather than being spelled out four times. A copy that drifted would
//! fail in the silent direction described above.
//!
//! # The two rules, and the order they must be applied in
//!
//! 1. **Under one of our own bases** — the configured CDN, or (for an object
//!    store) the vendor host anchored on our bucket. Handled by [`under_base`].
//! 2. **The bare resolver path** `/files/{key}`, which is what
//!    [`ferum_application::ports::file_url`] persists. Handled by
//!    [`strip_files_prefix`], and it must be tried **last**.
//!
//! Reversing that order is not cosmetic. A CDN can legitimately be mounted at a
//! path ending in `/files` — the natural choice for an operator moving from
//! database storage to an object store who wants existing URLs to keep their
//! shape — and a rule that scanned for `/files/` anywhere in the string would
//! then match the *base* and hand back a key still carrying the object-store
//! prefix. Reference counting would look that key up, find nothing, and report
//! success.

use ferum_application::ports::FILES_PREFIX;

/// Everything before the first `?` or `#`.
///
/// Signed URLs carry their authorization in the query string
/// (`?X-Goog-Algorithm=…`, `?X-Amz-Signature=…`), and a CDN may append a cache
/// buster. None of that is part of the key, and leaving it attached turns a
/// URL we minted into one we no longer recognise.
pub(super) fn without_query_or_fragment(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    &url[..end]
}

/// The bare resolver path `/files/{key}` — **anchored at the start of the URL**.
///
/// This is the form `ports::file_url` persists, so it is what avatars, site
/// config and the stored HTML of every post actually contain, whichever backend
/// is live. Reference counting resolves through it, so an adapter that stopped
/// recognising it would collect files that posts still display.
///
/// The anchor is what limits it to *our* origin. Matching `/files/` anywhere in
/// the string would claim `https://anyone-at-all.example.com/files/avatars/a.jpg`
/// as one of ours — and since `key_from_url` feeds `decrement_ref` and
/// `delete_by_key` (see `ThreadUseCase::set_thumbnail`, `UserUseCase` avatar and
/// cover replacement), an externally-hosted URL stored in one of those fields
/// could release a CAS reference it has nothing to do with.
/// A URL under our own CDN is absolute and is *not* handled here; it goes
/// through [`under_base`], which knows what our CDN is.
pub(super) fn strip_files_prefix(url: &str) -> Option<&str> {
    url.strip_prefix(FILES_PREFIX).filter(|key| !key.is_empty())
}

/// What sits under one of our own bases, and which of the two shapes it is.
///
/// The distinction matters only where an object-store prefix exists (`GCS_PREFIX`):
/// a resolver path was minted by `DatabaseStorageService` and never had one
/// applied, while an object name did.
pub(super) enum UnderBase<'a> {
    /// `{base}/files/{key}` — minted by `DatabaseStorageService` while this same
    /// CDN sat in front of it. The key follows verbatim.
    ResolverPath(&'a str),
    /// `{base}/{object name}` — this object-store adapter's own shape. Still
    /// carries whatever prefix the adapter applies on write.
    ObjectName(&'a str),
}

impl<'a> UnderBase<'a> {
    /// The inner string, for a backend that applies no prefix on write and so
    /// cannot tell the two shapes apart — nor needs to.
    pub(super) fn into_key(self) -> &'a str {
        match self {
            UnderBase::ResolverPath(s) | UnderBase::ObjectName(s) => s,
        }
    }
}

/// Splits `{base}/…` for a base with no trailing slash.
///
/// Returns `None` for an empty base so an unconfigured CDN cannot match every
/// relative URL in the database, and `None` when nothing follows the base.
pub(super) fn under_base<'a>(url: &'a str, base: &str) -> Option<UnderBase<'a>> {
    if base.is_empty() {
        return None;
    }
    let after_base = url.strip_prefix(base)?;

    // `/files/` is checked before the plain separator because it is the longer
    // match, and because no CAS key begins with `files/`: `cas_key` emits
    // `avatars`, `covers`, `thumbnails`, `post-attachments`, `products`,
    // `logos`, `favicons`, `theme-previews` or `plugin_{slug}`. So this
    // branch cannot swallow the leading segment of a legitimate object name.
    if let Some(key) = strip_files_prefix(after_base) {
        return Some(UnderBase::ResolverPath(key));
    }
    after_base
        .strip_prefix('/')
        .filter(|rest| !rest.is_empty())
        .map(UnderBase::ObjectName)
}

/// `{base}/{rest}` → `rest`, with no resolver-path interpretation.
///
/// For bases that only ever carry this adapter's own object names — the
/// Google-owned host prefixes, and the S3 endpoint's bucket root. Nothing else
/// has ever minted a `/files/` URL under those, so admitting one would only
/// widen what gets claimed.
///
/// Used by GCS and R2 — S3's single vendor base is an operator-set endpoint that
/// a CDN could plausibly shadow, so it goes through [`under_base`] instead.
#[cfg(any(feature = "gcs", feature = "r2"))]
pub(super) fn strip_base<'a>(url: &'a str, base: &str) -> Option<&'a str> {
    if base.is_empty() {
        return None;
    }
    url.strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|rest| !rest.is_empty())
}
