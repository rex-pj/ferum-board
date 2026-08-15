//! The datetime wire contract: RFC 3339 with an offset, both directions.
//!
//! A bare `<input type="datetime-local">` value is ZONELESS and serde rejects
//! it — which broke every temporary ban from the admin form while permanent bans
//! (sending `null`) kept working, so it read as intermittent.
//!
//! Clients must convert with `Ferum.localInputToIso()`. Pinned here is the
//! server half: a zoneless string stays rejected.

use chrono::{TimeZone, Utc};
use ferum_web::handlers::admin::api::users::BanUserRequest;
use ferum_web::view_models::report::TempBanRequest;

// ─── The shape the client must send ──────────────────────────────────────────

#[test]
fn a_zoneless_datetime_is_rejected_by_the_admin_ban_endpoint() {
    // Exactly what `<input type="datetime-local">` produces. If this ever starts
    // deserializing, the server has begun guessing a timezone on the user's
    // behalf — which is how "09:00" silently becomes 16:00 for a +07 admin.
    let body = r#"{"reason":"spam","banned_until":"2026-08-15T09:00"}"#;
    let parsed = serde_json::from_str::<BanUserRequest>(body);
    assert!(
        parsed.is_err(),
        "a datetime with no offset must not be accepted — the server cannot know \
         which zone the admin meant"
    );
}

#[test]
fn a_zoneless_datetime_is_rejected_by_the_mod_ban_endpoint() {
    // Same guarantee on the sibling endpoint. These two drifted apart once
    // already; the point of testing both is that they cannot drift again
    // without one of these failing.
    let body = r#"{"reason":"spam","until":"2026-08-15T09:00"}"#;
    assert!(
        serde_json::from_str::<TempBanRequest>(body).is_err(),
        "the moderator endpoint must hold the same contract as the admin one"
    );
}

#[test]
fn rfc3339_with_a_z_offset_is_accepted_and_keeps_the_instant() {
    let body = r#"{"reason":"spam","banned_until":"2026-08-15T09:00:00Z"}"#;
    let req: BanUserRequest = serde_json::from_str(body).expect("RFC 3339 must parse");
    assert_eq!(
        req.banned_until,
        Some(Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).unwrap())
    );
}

/// The real client path: a +07 admin types 09:00, the browser converts, the
/// server stores 02:00Z. This is the conversion `Ferum.localInputToIso()`
/// performs, asserted end to end so the expected instant is written down
/// somewhere rather than living only in a comment.
#[test]
fn a_non_utc_offset_is_normalised_to_the_same_instant() {
    let body = r#"{"reason":"spam","banned_until":"2026-08-15T09:00:00+07:00"}"#;
    let req: BanUserRequest = serde_json::from_str(body).expect("offset form must parse");
    assert_eq!(
        req.banned_until,
        Some(Utc.with_ymd_and_hms(2026, 8, 15, 2, 0, 0).unwrap()),
        "09:00 in +07 is 02:00 UTC — the offset must be applied, not discarded"
    );
}

#[test]
fn a_null_expiry_means_permanent_and_stays_valid() {
    // The case that kept working while every dated ban failed. Keeping it green
    // matters: it is what made the bug look intermittent.
    let req: BanUserRequest =
        serde_json::from_str(r#"{"reason":"spam","banned_until":null}"#).expect("null is valid");
    assert!(req.banned_until.is_none());

    let omitted: BanUserRequest =
        serde_json::from_str(r#"{"reason":"spam"}"#).expect("an omitted field is also permanent");
    assert!(omitted.banned_until.is_none());
}

// ─── The shape the server sends back ─────────────────────────────────────────

#[test]
fn instants_are_serialised_with_an_explicit_offset() {
    // The browser reads these with `new Date(...)`, which is only unambiguous
    // when the string carries a zone. A naive serialisation here would be
    // interpreted in the *device's* zone and shift every displayed timestamp.
    let instant = Utc.with_ymd_and_hms(2026, 8, 8, 23, 30, 0).unwrap();
    let json = serde_json::to_string(&instant).expect("serialise");
    assert!(
        json.contains('Z') || json.contains("+00:00"),
        "a serialised instant must name its offset, got {json}"
    );
    // And it must survive the round trip unchanged.
    let back: chrono::DateTime<Utc> = serde_json::from_str(&json).expect("round trip");
    assert_eq!(back, instant);
}
