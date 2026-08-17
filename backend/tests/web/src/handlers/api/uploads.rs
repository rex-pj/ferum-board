//! Tests for the `/files/{key}` key guard — `serve()` needs a full `AppState`
//! and is not constructible here, so only its pure decision function is covered.
//!
//! The guard matters because `public_url` concatenates `{base}/{key}`, and under
//! the path-style URLs S3 and GCS use, a key containing `..` normalises to a
//! **different bucket**. Harmless until the key could reach a `Location` header.

use std::collections::HashMap;

use ferum_domain::models::user::TrustLevel;
use ferum_domain::AuthUser;
use ferum_web::handlers::api::uploads::{
    cache_policy, is_safe_key, may_be_staged, resolve_stored_file, FileDisposition, Viewer,
};
use uuid::Uuid;

/// A moderator or admin. There is no constructor for this on `Viewer` on purpose
/// — production must build it from a resolved permission set via
/// `Viewer::from_auth`, never by asserting staff-ness at a call site.
fn staff(id: Uuid) -> Viewer {
    Viewer {
        id: Some(id),
        is_staff: true,
    }
}

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
        resolve_stored_file(STAGED, 0, Some(owner), false, Viewer::member(stranger)),
        FileDisposition::NotFound
    );
}

#[test]
fn a_guest_is_refused_a_staged_attachment() {
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(Uuid::new_v4()), false, Viewer::guest()),
        FileDisposition::NotFound
    );
    // ...and with bytes present, i.e. under database storage, where the check
    // did work before.
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(Uuid::new_v4()), true, Viewer::guest()),
        FileDisposition::NotFound
    );
}

#[test]
fn a_staged_attachment_with_no_owner_is_reachable_by_no_member() {
    // `uploaded_by_id` is nullable (ON DELETE SET NULL). A logged-in viewer must
    // not match a NULL owner just because both are "absent".
    assert_eq!(
        resolve_stored_file(STAGED, 0, None, false, Viewer::member(Uuid::new_v4())),
        FileDisposition::NotFound
    );
    assert_eq!(
        resolve_stored_file(STAGED, 0, None, false, Viewer::guest()),
        FileDisposition::NotFound
    );
    // Staff are the exception, and this is the case that makes the exception
    // necessary rather than convenient: deleting the uploader's account NULLs
    // this column, so without it an abandoned upload becomes permanently
    // unreviewable while still occupying the store.
    assert_eq!(
        resolve_stored_file(STAGED, 0, None, false, staff(Uuid::new_v4())),
        FileDisposition::StagedRedirect
    );
}

#[test]
fn the_uploader_reaches_their_own_staged_attachment() {
    let owner = Uuid::new_v4();
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), true, Viewer::member(owner)),
        FileDisposition::StagedBytes,
        "the composer preview must work under database storage"
    );
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), false, Viewer::member(owner)),
        FileDisposition::StagedRedirect,
        "and under an object store"
    );
}

#[test]
fn staff_reach_a_staged_attachment_they_did_not_upload() {
    // Soft-deleting a post drives its attachments to ref_count = 0, and those
    // objects are deliberately retained as the evidence for the removal. While
    // the uploader was the only accepted viewer, that evidence was readable by
    // the account that posted it and by nobody else — the moderator who removed
    // the post included. `/admin/storage` is where that became visible: every
    // retained-attachment thumbnail on it was a 404.
    let owner = Uuid::new_v4();
    let moderator = Uuid::new_v4();
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), true, staff(moderator)),
        FileDisposition::StagedBytes
    );
    assert_eq!(
        resolve_stored_file(STAGED, 0, Some(owner), false, staff(moderator)),
        FileDisposition::StagedRedirect
    );
}

#[test]
fn staff_access_does_not_make_a_staged_attachment_cacheable() {
    // Both staff arms must land on the Staged* variants, never the Published*
    // ones. `serve` keys `private, no-store` and the absence of an ETag off that
    // distinction alone, so returning a Published variant here would let a shared
    // cache hand a removed post's image to the next person through it.
    for has_bytes in [true, false] {
        let d = resolve_stored_file(STAGED, 0, Some(Uuid::new_v4()), has_bytes, staff(Uuid::new_v4()));
        assert!(
            matches!(
                d,
                FileDisposition::StagedBytes | FileDisposition::StagedRedirect
            ),
            "staff got {d:?}, which `serve` would cache"
        );
    }
}

#[test]
fn publishing_an_attachment_makes_it_public() {
    // Once some post embeds it, ref_count > 0 and it is ordinary content.
    let owner = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    assert_eq!(
        resolve_stored_file(STAGED, 1, Some(owner), false, Viewer::member(stranger)),
        FileDisposition::PublishedRedirect
    );
    assert_eq!(
        resolve_stored_file(STAGED, 1, Some(owner), true, Viewer::guest()),
        FileDisposition::PublishedBytes
    );
}

#[test]
fn a_negative_ref_count_still_counts_as_staged() {
    // Ref counting is decrement-on-unlink; a bookkeeping bug that drove the
    // count below zero must fail closed, not expose the file.
    assert_eq!(
        resolve_stored_file(STAGED, -1, Some(Uuid::new_v4()), false, Viewer::guest()),
        FileDisposition::NotFound
    );
    // Still staged for staff too — they get the private, uncacheable arm, not a
    // redirect a cache could keep.
    assert_eq!(
        resolve_stored_file(STAGED, -1, Some(Uuid::new_v4()), false, staff(Uuid::new_v4())),
        FileDisposition::StagedRedirect
    );
}

#[test]
fn a_zero_ref_count_outside_the_attachment_namespace_is_not_staged() {
    // An avatar between `put` and `upsert_and_ref` momentarily has ref_count 0.
    // Treating that as staged would 404 avatars for everyone but their owner.
    assert_eq!(
        resolve_stored_file(PUBLISHED_AVATAR, 0, Some(Uuid::new_v4()), true, Viewer::guest()),
        FileDisposition::PublishedBytes
    );
    assert_eq!(
        resolve_stored_file(PUBLISHED_AVATAR, 0, None, false, Viewer::guest()),
        FileDisposition::PublishedRedirect
    );
}

// ─── cache_policy — the rule a shared cache would otherwise break ────────────
//
// These four dispositions used to be literal strings inside `serve`'s match arms,
// where nothing could reach them: `serve` needs an `AppState`. The rule they
// encode is that a *staged* response is never storable, and it protects more
// viewers than it used to now that staff reach staged attachments.

const EVERY_DISPOSITION: [FileDisposition; 5] = [
    FileDisposition::NotFound,
    FileDisposition::StagedBytes,
    FileDisposition::StagedRedirect,
    FileDisposition::PublishedBytes,
    FileDisposition::PublishedRedirect,
];

#[test]
fn no_staged_response_may_be_stored_by_any_cache() {
    // THE invariant. A staged attachment is authorized per viewer, so a cache
    // keeping it would serve one person's private upload to the next request that
    // came through — and since `/files/` sits behind a CDN in every non-trivial
    // deployment, "any cache" is not hypothetical.
    for disposition in [FileDisposition::StagedBytes, FileDisposition::StagedRedirect] {
        let cache = cache_policy(disposition).expect("a staged response has a body");
        assert_eq!(
            cache.cache_control, "private, no-store",
            "{disposition:?} must be unstorable"
        );
        assert!(
            !cache.etag,
            "{disposition:?} must carry no ETag — it invites the revalidation that \
             `serve`'s 304 short-circuit answers from the key alone, and that \
             short-circuit deliberately never runs for a staged key"
        );
    }
}

#[test]
fn only_a_published_body_gets_an_etag() {
    // An ETag is a promise the bytes behind this key never change. True of a
    // published CAS object; a redirect's *location* moves when the backend does.
    for disposition in EVERY_DISPOSITION {
        let etag = cache_policy(disposition).is_some_and(|c| c.etag);
        assert_eq!(
            etag,
            disposition == FileDisposition::PublishedBytes,
            "ETag on {disposition:?}"
        );
    }
}

#[test]
fn a_published_redirect_is_cacheable_but_never_immutable() {
    // Content behind a CAS key is immutable; its *location* is not — it moves when
    // the backend, bucket or CDN changes. `immutable` here would pin every visitor
    // to the old origin until they cleared their cache.
    let cache = cache_policy(FileDisposition::PublishedRedirect).expect("has a policy");
    assert!(cache.cache_control.starts_with("public, max-age="));
    assert!(
        !cache.cache_control.contains("immutable"),
        "a location must stay revalidatable: {}",
        cache.cache_control
    );
    assert!(!cache.etag);
}

#[test]
fn a_published_body_may_be_cached_forever() {
    let cache = cache_policy(FileDisposition::PublishedBytes).expect("has a policy");
    assert_eq!(cache.cache_control, "public, max-age=31536000, immutable");
    assert!(cache.etag);
}

#[test]
fn nothing_to_serve_means_nothing_to_cache() {
    // `serve` keys its 404 arm off this `None`, so a policy appearing here would
    // route a refused request into the redirect arm instead.
    assert!(cache_policy(FileDisposition::NotFound).is_none());
}

#[test]
fn every_disposition_a_viewer_can_reach_has_a_policy() {
    // Guards the pairing rather than the values: a sixth variant added to
    // `FileDisposition` without a `cache_policy` arm would not compile, but one
    // added with a *wrong* arm would — and `resolve_stored_file` is where new
    // variants come from.
    for disposition in EVERY_DISPOSITION {
        assert_eq!(
            cache_policy(disposition).is_some(),
            disposition != FileDisposition::NotFound,
            "policy presence for {disposition:?}"
        );
    }
}

// ─── Viewer::from_auth — which permission set counts as staff ────────────────

fn auth_user(global: &[&str], scoped: &[&str]) -> AuthUser {
    let mut category_permissions = HashMap::new();
    if !scoped.is_empty() {
        category_permissions.insert(
            Uuid::new_v4(),
            scoped.iter().map(|s| s.to_string()).collect(),
        );
    }
    AuthUser {
        id: Uuid::new_v4(),
        username: "someone".into(),
        display_name: None,
        avatar_url: None,
        trust_level: TrustLevel::Member,
        is_banned: false,
        banned_until: None,
        permissions: global.iter().map(|s| s.to_string()).collect(),
        category_permissions,
    }
}

#[test]
fn a_category_scoped_moderator_counts_as_staff() {
    // The normal shape of the role: `moderation.view_reports` granted per
    // category, nothing global. `has_perm` alone would refuse them, and this
    // endpoint has only a CAS key — nothing in it names the category — so the
    // check has to be `has_perm_any_category`.
    let user = auth_user(&[], &["moderation.view_reports"]);
    assert!(Viewer::from_auth(Some(&user)).is_staff);
}

#[test]
fn an_admin_holding_only_config_counts_as_staff() {
    // `/admin/storage` is gated on `admin.config`, so whoever can open the page
    // must be able to see the thumbnails on it.
    let user = auth_user(&["admin.config"], &[]);
    assert!(Viewer::from_auth(Some(&user)).is_staff);
}

#[test]
fn an_ordinary_member_is_not_staff() {
    // Including permissions that sound adjacent but are not moderation powers:
    // filing a report is something every member may do.
    let user = auth_user(&["post.create", "report.create", "file.upload"], &[]);
    let viewer = Viewer::from_auth(Some(&user));
    assert!(!viewer.is_staff);
    assert_eq!(viewer.id, Some(user.id));
}

#[test]
fn from_auth_on_none_is_a_guest() {
    assert_eq!(Viewer::from_auth(None), Viewer::guest());
}

#[test]
fn a_guest_is_never_staff() {
    // `is_staff` is derived, never passed in from a request. A guest has no
    // permission set to read, so the only correct answer is `false` — and the
    // `Default` impl is what guarantees it rather than a branch that could be
    // reordered.
    assert!(!Viewer::guest().is_staff);
    assert_eq!(Viewer::guest().id, None);
    assert!(!Viewer::member(Uuid::new_v4()).is_staff);
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
