//! `EmailNotificationPrefs` — the rule that decides who gets mailed.
//!
//! The asymmetry these tests pin is the whole migration story: an account created
//! before this feature has `{}` and must stay silent, while a new account gets both
//! keys written explicitly and is opted in. Reading a missing key as `true` would
//! mail an entire existing community on the first deploy over something none of
//! them agreed to — the one mistake in this feature that cannot be undone, because
//! it is the sending domain's reputation that pays.

use ferum_domain::models::EmailNotificationPrefs;
use serde_json::json;

#[test]
fn an_empty_object_is_every_flag_off() {
    // The exact shape every pre-existing `user_preferences` row has.
    let prefs = EmailNotificationPrefs::from_json(&json!({}));
    assert!(!prefs.reply);
    assert!(!prefs.mention);
    assert!(prefs.all_off());
}

#[test]
fn an_absent_key_is_off_even_when_the_other_is_on() {
    // A partially-written object must not have its missing half inferred.
    let prefs = EmailNotificationPrefs::from_json(&json!({ "reply": true }));
    assert!(prefs.reply);
    assert!(!prefs.mention);
}

#[test]
fn explicit_flags_are_honoured() {
    let prefs = EmailNotificationPrefs::from_json(&json!({
        "reply": true,
        "mention": false,
    }));
    assert!(prefs.reply);
    assert!(!prefs.mention);
    assert!(!prefs.all_off());
}

#[test]
fn a_non_boolean_value_reads_as_off() {
    // Fails in the safe direction, so a hand-edited row or a future schema change
    // cannot turn mail *on* by accident.
    for value in [
        json!({ "reply": "true" }),
        json!({ "reply": 1 }),
        json!({ "reply": null }),
        json!({ "reply": {} }),
    ] {
        let prefs = EmailNotificationPrefs::from_json(&value);
        assert!(!prefs.reply, "{value} should read as off");
    }
}

#[test]
fn a_non_object_json_value_reads_as_off() {
    // `email_notifications` is JSONB and nothing at the type level stops a scalar
    // getting in there.
    for value in [json!(null), json!(true), json!("reply"), json!([1, 2])] {
        let prefs = EmailNotificationPrefs::from_json(&value);
        assert!(prefs.all_off(), "{value} should read as all off");
    }
}

#[test]
fn opted_in_is_what_a_new_account_gets() {
    // Asserted through the stored representation rather than the fields: that is
    // what `AuthUseCase::register` actually persists, and a field assertion on a
    // `const` is folded away at compile time and tests nothing.
    let stored = EmailNotificationPrefs::OPTED_IN.to_json();
    assert_eq!(stored["reply"], json!(true));
    assert_eq!(stored["mention"], json!(true));
    assert!(!EmailNotificationPrefs::from_json(&stored).all_off());
}

#[test]
fn opted_out_is_what_the_unsubscribe_link_writes() {
    let stored = EmailNotificationPrefs::OPTED_OUT.to_json();
    assert!(EmailNotificationPrefs::from_json(&stored).all_off());
}

#[test]
fn the_default_is_off_not_opted_in() {
    // `UserPreferences::default()` is what a user with no preferences row resolves
    // to, which is every account predating this feature. If `Default` ever drifted
    // to mean "opted in", that alone would start mailing them.
    assert!(EmailNotificationPrefs::default().all_off());
}

#[test]
fn to_json_round_trips_through_from_json() {
    // The stored representation has to be readable by the reader — they are written
    // and read in different crates (auth use case writes, event bus reads).
    for prefs in [
        EmailNotificationPrefs::OPTED_IN,
        EmailNotificationPrefs::OPTED_OUT,
        EmailNotificationPrefs { reply: true, mention: false },
        EmailNotificationPrefs { reply: false, mention: true },
    ] {
        assert_eq!(EmailNotificationPrefs::from_json(&prefs.to_json()), prefs);
    }
}

#[test]
fn to_json_writes_both_keys_explicitly() {
    // Explicitness is the mechanism, not a detail: a written `false` is what
    // distinguishes "chose not to" from "never had the choice", and only the second
    // may be changed by a future default.
    let value = EmailNotificationPrefs::OPTED_OUT.to_json();
    assert_eq!(value["reply"], json!(false));
    assert_eq!(value["mention"], json!(false));
    assert_eq!(value.as_object().unwrap().len(), 2);
}
