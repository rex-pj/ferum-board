//! Tests for the `/files/{key}` key guard — `serve()` needs a full `AppState`
//! and is not constructible here, so only its pure decision function is covered.
//!
//! The guard matters because `public_url` concatenates `{base}/{key}`, and under
//! the path-style URLs S3 and GCS use, a key containing `..` normalises to a
//! **different bucket**. Harmless until the key could reach a `Location` header.

use ferum_web::handlers::api::uploads::{
    is_safe_key, may_be_staged, resolve_stored_file, FileDisposition,
};
use uuid::Uuid;

/// Exactly the shapes `storage_utils::cas_key` emits: `{prefix}/{hex}.{ext}`,
/// including the hyphenated attachment namespace and a plugin's dynamic prefix.
#[test]
fn every_key_the_application_actually_mints_is_accepted() {
    for key in [
        "avatars/0123456789abcdef0123456789abcdef.png",
        "post-attachments/fedcba9876543210fedcba9876543210.jpg",
        "plugin_community-polls/aaaabbbbccccddddeeeeffff00001111.webp",
        "theme-previews/11112222333344445555666677778888.webp",
        "products/00000000000000000000000000000000.gif",
    ] {
        assert!(is_safe_key(key), "legitimate key rejected: {key}");
    }
}

#[test]
fn traversal_is_refused() {
    // `https://storage.googleapis.com/our-bucket/../other-bucket/x.png`
    // normalises to `https://storage.googleapis.com/other-bucket/x.png`.
    for key in [
        "../other-bucket/x.png",
        "avatars/../../other-bucket/x.png",
        "..",
    ] {
        assert!(!is_safe_key(key), "traversal accepted: {key}");
    }
}

#[test]
fn an_absolute_or_protocol_relative_key_is_refused() {
    // `//evil.test/x.png` as a Location value is a protocol-relative URL to
    // another host entirely.
    assert!(!is_safe_key("/etc/passwd"));
    assert!(!is_safe_key("//evil.test/x.png"));
    assert!(!is_safe_key("avatars//x.png"));
}

#[test]
fn backslashes_are_refused() {
    // Some clients and proxies normalise `\` to `/`, so allowing it would leave
    // a second spelling of every rule above.
    assert!(!is_safe_key("avatars\\..\\x.png"));
}

#[test]
fn control_characters_are_refused() {
    // A newline in a key would be a header-injection attempt against `Location`.
    assert!(!is_safe_key("avatars/x.png\r\nSet-Cookie: a=b"));
    assert!(!is_safe_key("avatars/\0.png"));
}

#[test]
fn an_empty_key_is_refused() {
    assert!(!is_safe_key(""));
}

// ─── Disposition: the ordering that used to be wrong ─────────────────────────
//
// `resolve_stored_file` exists as a separate pure function precisely so this can
// be checked. The staged test must run before the bytes test: it once ran after,
// so under an object store — where `data` is always NULL — the redirect fired
// first and the ownership check was unreachable code. Every staged attachment's
// location was handed to whoever asked for it.
//
// `has_bytes = false` in each case below is what an object-store deployment
// always looks like, which is why those rows carry the whole regression risk.

const STAGED: &str = "post-attachments/fedcba9876543210fedcba9876543210.jpg";
const PUBLISHED_AVATAR: &str = "avatars/0123456789abcdef0123456789abcdef.png";

#[test]
fn a_stranger_is_refused_a_staged_attachment_even_when_its_bytes_are_elsewhere() {
    // THE regression test. With the old ordering this returned a redirect.
    let owner = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), false, Some(stranger)),
        FileDisposition::NotFound
    );
}

#[test]
fn a_guest_is_refused_a_staged_attachment() {
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(Uuid::new_v4()), false, None),
        FileDisposition::NotFound
    );
    // ...and with bytes present, i.e. under database storage, where the check
    // did work before.
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(Uuid::new_v4()), true, None),
        FileDisposition::NotFound
    );
}

#[test]
fn a_staged_attachment_with_no_owner_is_reachable_by_nobody() {
    // `uploaded_by_id` is nullable (ON DELETE SET NULL). A logged-in viewer must
    // not match a NULL owner just because both are "absent".
    assert_eq!(
        resolve_stored_file(STAGED, 0, None, false, Some(Uuid::new_v4())),
        FileDisposition::NotFound
    );
    assert_eq!(
        resolve_stored_file(STAGED, 0, None, false, None),
        FileDisposition::NotFound
    );
}

#[test]
fn the_uploader_reaches_their_own_staged_attachment() {
    let owner = Uuid::new_v4();
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), true, Some(owner)),
        FileDisposition::StagedBytes,
        "the composer preview must work under database storage"
    );
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), false, Some(owner)),
        FileDisposition::StagedRedirect,
        "and under an object store"
    );
}

#[test]
fn publishing_an_attachment_makes_it_public() {
    // Once some post embeds it, ref_count > 0 and it is ordinary content.
    let owner = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    assert_eq!(
        resolve_stored_file(STAGED, 1, Some(owner), false, Some(stranger)),
        FileDisposition::PublishedRedirect
    );
    assert_eq!(
        resolve_stored_file(STAGED, 1, Some(owner), true, None),
        FileDisposition::PublishedBytes
    );
}

#[test]
fn a_negative_ref_count_still_counts_as_staged() {
    // Ref counting is decrement-on-unlink; a bookkeeping bug that drove the
    // count below zero must fail closed, not expose the file.
    assert_eq!(
        resolve_stored_file(STAGED, -1, Some(Uuid::new_v4()), false, None),
        FileDisposition::NotFound
    );
}

#[test]
fn a_zero_ref_count_outside_the_attachment_namespace_is_not_staged() {
    // An avatar between `put` and `upsert_and_ref` momentarily has ref_count 0.
    // Treating that as staged would 404 avatars for everyone but their owner.
    assert_eq!(
        resolve_stored_file(PUBLISHED_AVATAR, 0, Some(Uuid::new_v4()), true, None),
        FileDisposition::PublishedBytes
    );
    assert_eq!(
        resolve_stored_file(PUBLISHED_AVATAR, 0, None, false, None),
        FileDisposition::PublishedRedirect
    );
}

#[test]
fn only_the_attachment_namespace_can_be_staged() {
    // The three decisions keyed on this must agree, which is why it is one
    // named function rather than three inline `starts_with` calls.
    assert!(may_be_staged(STAGED));
    for key in [
        PUBLISHED_AVATAR,
        "covers/a.png",
        "products/a.png",
        "plugin_polls/a.png",
        // Not a prefix match: a different namespace that merely begins the same.
        "post-attachments-old/a.png",
    ] {
        assert_eq!(may_be_staged(key), key == STAGED, "may_be_staged({key})");
    }
}
