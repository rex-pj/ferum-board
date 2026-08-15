//! URL-shape rules shared by every `StorageService` backend — here, not copied
//! four times, because a drifted copy fails silently.
//!
//! `key_from_url` mis-recognising a URL is never an error: it reports "not
//! ours", so a reference is never taken or never released and files leak or get
//! collected out from under live posts.
//!
//! **Rule order is load-bearing.** [`under_base`] (CDN or vendor host) FIRST,
//! then [`strip_files_prefix`] LAST and anchored — a CDN legitimately mounted at
//! a path ending in `/files` would otherwise match inside the base and yield a
//! key still carrying the object-store prefix.

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
/// The form `ports::file_url` persists, so every adapter must recognise it or
/// collect files that posts still display.
///
/// The anchor limits it to OUR origin: matching `/files/` anywhere would claim
/// `https://anyone.example.com/files/a.jpg`, and since this feeds
/// `decrement_ref`, a foreign URL could release someone else's reference.
/// Absolute CDN URLs go through [`under_base`] instead.
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
