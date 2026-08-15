//! `SecretCipher` — the guarantees the two encrypted columns rest on.
//!
//! Needs no database and no key material from the environment: every test here
//! supplies its own key, so the whole file runs on a machine with nothing set up.

use ferum_infrastructure::crypto::{
    plugin_config_aad, site_config_aad, SecretCipher, ENCRYPTED_CONFIG_KEYS, SEALED_PREFIX,
    WEBHOOK_SECRET_AAD,
};

/// Two distinct, valid keys. Fixed rather than random so a failure is reproducible.
const KEY_A: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
const KEY_B: &str = "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";

fn cipher(current: &str, previous: Option<&str>) -> SecretCipher {
    SecretCipher::from_hex(current, previous).expect("valid key")
}

const AAD: &[u8] = b"site_config:smtp_pass";

#[test]
fn seals_and_opens_round_trip() {
    let c = cipher(KEY_A, None);
    let sealed = c.seal(AAD, "hunter2").unwrap();
    assert!(sealed.starts_with(SEALED_PREFIX));
    let opened = c.open(AAD, &sealed).unwrap();
    assert_eq!(opened.value, "hunter2");
    assert!(
        !opened.needs_reseal,
        "a value sealed under the current key must not be flagged for re-sealing, \
         or the startup sweep rewrites every row on every boot"
    );
}

#[test]
fn identical_plaintext_produces_different_ciphertext() {
    let c = cipher(KEY_A, None);
    let a = c.seal(AAD, "same").unwrap();
    let b = c.seal(AAD, "same").unwrap();
    // A fresh random nonce per seal. Were these equal, two rows holding the same
    // password would be visibly identical in a dump.
    assert_ne!(a, b);
    assert_eq!(c.open(AAD, &a).unwrap().value, "same");
    assert_eq!(c.open(AAD, &b).unwrap().value, "same");
}

#[test]
fn tampering_with_any_byte_fails_authentication() {
    let c = cipher(KEY_A, None);
    let sealed = c.seal(AAD, "hunter2").unwrap();
    let body_start = SEALED_PREFIX.len();

    // Flip one hex digit at a time across the whole envelope — nonce, ciphertext
    // and tag alike. Every position must fail; none may yield a value.
    for i in body_start..sealed.len() {
        let mut bytes = sealed.clone().into_bytes();
        bytes[i] = if bytes[i] == b'0' { b'1' } else { b'0' };
        let mutated = String::from_utf8(bytes).unwrap();
        if mutated == sealed {
            continue;
        }
        assert!(
            c.open(AAD, &mutated).is_err(),
            "a single altered byte at offset {i} was accepted"
        );
    }
}

#[test]
fn a_wrong_key_fails_rather_than_returning_garbage() {
    let sealed = cipher(KEY_A, None).seal(AAD, "hunter2").unwrap();
    // The property the whole design rests on. `webhooks.secret` is an HMAC key: a
    // cipher without authentication would hand back 32 bytes of nonsense, every
    // subscriber would reject the signature, and `record_failure` would tick up
    // with nothing anywhere saying why.
    let err = cipher(KEY_B, None).open(AAD, &sealed);
    assert!(err.is_err());
}

#[test]
fn aad_mismatch_fails() {
    let c = cipher(KEY_A, None);
    // Sealed as a site_config value, opened as a webhook secret: the ciphertext is
    // intact and the key is right, and it must still fail. This is what stops a
    // sealed value being moved between columns at the SQL level.
    let sealed = c.seal(AAD, "hunter2").unwrap();
    assert!(c.open(WEBHOOK_SECRET_AAD, &sealed).is_err());
}

#[test]
fn aad_is_distinct_per_config_key() {
    let c = cipher(KEY_A, None);
    let sealed = c.seal(&site_config_aad("smtp_pass"), "hunter2").unwrap();
    assert!(c.open(&site_config_aad("site_name"), &sealed).is_err());
}

#[test]
fn unprefixed_plaintext_is_returned_unchanged_and_flagged() {
    let c = cipher(KEY_A, None);
    // The backward-compatibility path: this is what makes enabling encryption on
    // an existing database a no-op at read time.
    let opened = c.open(AAD, "legacy-plaintext-password").unwrap();
    assert_eq!(opened.value, "legacy-plaintext-password");
    assert!(opened.needs_reseal);
}

#[test]
fn is_sealed_recognises_only_the_versioned_prefix() {
    assert!(SecretCipher::is_sealed("enc:v1:00ff"));
    assert!(!SecretCipher::is_sealed("enc:v2:00ff"));
    assert!(!SecretCipher::is_sealed("enc:00ff"));
    assert!(!SecretCipher::is_sealed(""));
    // A password that merely *mentions* the word must not be mistaken for one.
    assert!(!SecretCipher::is_sealed("my enc:v1: password"));
}

#[test]
fn key_parsing_rejects_everything_that_is_not_64_hex_chars() {
    let short = "0".repeat(63);
    let long = "0".repeat(65);
    let non_hex = "z".repeat(64);
    for bad in [short.as_str(), long.as_str(), non_hex.as_str(), ""] {
        assert!(
            SecretCipher::from_hex(bad, None).is_err(),
            "accepted an invalid key: {bad:?}"
        );
    }
    // Case-insensitive, since `openssl rand -hex` and a hand-typed key differ.
    assert!(SecretCipher::from_hex(&KEY_A.to_uppercase(), None).is_ok());
    // Surrounding whitespace is common in an env file and is not an error.
    assert!(SecretCipher::from_hex(&format!("  {KEY_A}\n"), None).is_ok());
}

#[test]
fn an_invalid_previous_key_is_rejected_too() {
    // Silently ignoring a malformed previous key would make a rotation look
    // complete while the old values stayed unreadable.
    assert!(SecretCipher::from_hex(KEY_A, Some("nonsense")).is_err());
    // Blank means "no previous key", not "an invalid one".
    assert!(SecretCipher::from_hex(KEY_A, Some("   ")).is_ok());
}

#[test]
fn a_key_parsing_error_never_echoes_the_key() {
    let secretish = "z".repeat(64);
    // `unwrap_err` would need `SecretCipher: Debug`, and giving a cipher a derived
    // Debug is how key material ends up in a log line. Match instead.
    let Err(err) = SecretCipher::from_hex(&secretish, None) else {
        panic!("a non-hex key should be rejected");
    };
    let err = err.to_string();
    assert!(!err.contains(&secretish));
    // Still has to be actionable.
    assert!(err.contains("SECRET_ENCRYPTION_KEY"));
}

#[test]
fn previous_key_opens_and_flags_for_reseal_while_current_key_seals() {
    // The whole rotation mechanism: opened under `previous`, re-sealed under
    // `current` by the startup sweep, which acts on exactly this flag.
    let old = cipher(KEY_A, None);
    let sealed_with_old = old.seal(AAD, "hunter2").unwrap();

    let rotating = cipher(KEY_B, Some(KEY_A));
    let opened = rotating.open(AAD, &sealed_with_old).unwrap();
    assert_eq!(opened.value, "hunter2");
    assert!(opened.needs_reseal);

    // And what it writes back is readable by the new key alone, so
    // SECRET_ENCRYPTION_KEY_PREVIOUS can be dropped afterwards.
    let resealed = rotating.seal(AAD, &opened.value).unwrap();
    let after = cipher(KEY_B, None).open(AAD, &resealed).unwrap();
    assert_eq!(after.value, "hunter2");
    assert!(!after.needs_reseal);
}

#[test]
fn a_malformed_envelope_is_an_error_not_a_panic() {
    let c = cipher(KEY_A, None);
    for bad in [
        "enc:v1:",                 // empty body
        "enc:v1:zzzz",             // not hex
        "enc:v1:00112233",         // shorter than the nonce
        &format!("enc:v1:{}", "0".repeat(48)), // nonce only, no ciphertext or tag
    ] {
        assert!(c.open(AAD, bad).is_err(), "accepted malformed envelope {bad:?}");
    }
}

#[test]
fn a_512_char_secret_seals_to_ascii_that_fits_a_text_column() {
    // 512 is the validated maximum for a webhook secret. Both columns are TEXT, so
    // this cannot overflow — the test pins the shape (hex ASCII, ~2x plus overhead)
    // so a future envelope change that bloated it would be noticed here rather
    // than by a failing INSERT in production.
    let c = cipher(KEY_A, None);
    let secret = "x".repeat(512);
    let sealed = c.seal(WEBHOOK_SECRET_AAD, &secret).unwrap();
    assert!(sealed.is_ascii());
    assert!(sealed.len() < 1200, "sealed length was {}", sealed.len());
    assert_eq!(c.open(WEBHOOK_SECRET_AAD, &sealed).unwrap().value, secret);
}

#[test]
fn non_ascii_and_empty_values_round_trip() {
    let c = cipher(KEY_A, None);
    // A password with multi-byte characters must survive the UTF-8 round trip.
    let tricky = "mật-khẩu-🔐";
    let sealed = c.seal(AAD, tricky).unwrap();
    assert_eq!(c.open(AAD, &sealed).unwrap().value, tricky);

    // The cipher itself will seal an empty string; the repositories decline to,
    // because a blank secret means "not set". Asserted so the two layers' division
    // of responsibility stays visible.
    let sealed_empty = c.seal(AAD, "").unwrap();
    assert_eq!(c.open(AAD, &sealed_empty).unwrap().value, "");
}

#[test]
fn smtp_pass_is_the_encrypted_config_key() {
    // Guards the policy list itself. Removing a key from it would leave already
    // sealed rows being handed to callers as literal `enc:v1:…` text — the one
    // change to this list that is not backward compatible.
    assert!(ENCRYPTED_CONFIG_KEYS.contains(&"smtp_pass"));
}

// ─── Plugin config AAD ───────────────────────────────────────────────────────

#[test]
fn a_plugin_secret_cannot_be_moved_between_plugins() {
    let c = cipher(KEY_A, None);
    let sealed = c
        .seal(
            &plugin_config_aad("discord-notifier", "webhook_url"),
            "https://discord.com/api/webhooks/1/abc",
        )
        .unwrap();

    // The same field name under a different plugin. Without the slug in the AAD
    // this would decrypt cleanly, letting anyone with database access graft one
    // install's credential onto another plugin and have it used.
    assert!(
        c.open(&plugin_config_aad("other-plugin", "webhook_url"), &sealed)
            .is_err(),
        "a sealed plugin credential must not open under a different plugin slug"
    );
}

#[test]
fn a_plugin_secret_cannot_be_moved_between_fields() {
    let c = cipher(KEY_A, None);
    let sealed = c
        .seal(&plugin_config_aad("acme", "api_key"), "sk-live-1")
        .unwrap();
    assert!(
        c.open(&plugin_config_aad("acme", "fallback_key"), &sealed)
            .is_err(),
        "a sealed plugin credential must not open under a different config key"
    );
}

#[test]
fn plugin_config_aad_is_distinct_from_the_other_two_tables() {
    let c = cipher(KEY_A, None);
    let sealed = c
        .seal(&plugin_config_aad("acme", "smtp_pass"), "hunter2")
        .unwrap();

    // A plugin that happens to name a field `smtp_pass` must not produce a value
    // interchangeable with the site's actual SMTP password, in either direction.
    assert!(c.open(&site_config_aad("smtp_pass"), &sealed).is_err());
    assert!(c.open(WEBHOOK_SECRET_AAD, &sealed).is_err());

    let site_sealed = c.seal(&site_config_aad("smtp_pass"), "hunter2").unwrap();
    assert!(c
        .open(&plugin_config_aad("acme", "smtp_pass"), &site_sealed)
        .is_err());
}

#[test]
fn plaintext_plugin_config_passes_through_and_is_flagged() {
    let c = cipher(KEY_A, None);
    // The lazy-migration path: a plugin configured before encryption was turned
    // on keeps working, and the sweep reseals it. Were this an error instead,
    // setting SECRET_ENCRYPTION_KEY would break every already-installed plugin.
    let opened = c
        .open(
            &plugin_config_aad("discord-notifier", "webhook_url"),
            "https://discord.com/api/webhooks/1/abc",
        )
        .unwrap();
    assert_eq!(opened.value, "https://discord.com/api/webhooks/1/abc");
    assert!(opened.needs_reseal);
}
