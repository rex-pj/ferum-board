//! `email_log_key` — the masking every log line touching an email address goes
//! through.
//!
//! The rule it enforces is easy to regress by writing what looks like a helpful
//! log line, so the properties here are stated as behaviour rather than as a
//! fixed expected string: what must never appear, and what must stay useful.

use ferum_application::shared::email_log_key;

#[test]
fn the_local_part_never_survives() {
    // The whole point. A login endpoint sees addresses belonging to people with
    // no account, and users paste passwords into the field, so the identifying
    // half must not reach the log under any input shape.
    for raw in [
        "alice@example.com",
        "Alice.Smith+tag@Example.COM",
        "  bob@example.com  ",
        "\"quoted@local\"@example.com",
    ] {
        let masked = email_log_key(raw);
        let local = raw.trim().to_lowercase();
        let local = local.rsplit_once('@').map(|(l, _)| l.to_string()).unwrap();
        assert!(
            !masked.contains(&local),
            "{raw} → {masked} still contains the local part {local:?}"
        );
    }
}

#[test]
fn the_domain_is_kept_so_enumeration_of_one_org_is_visible() {
    // "Someone is walking @ourcorp.com" is a different operational event from
    // scattered noise, and a domain shared by millions of mailboxes identifies
    // nobody by itself.
    assert!(email_log_key("alice@example.com").ends_with("@example.com"));
    // The last `@` wins: a quoted local part may legally contain one, and the
    // domain is what follows the final separator.
    assert!(email_log_key("\"a@b\"@example.com").ends_with("@example.com"));
}

#[test]
fn a_value_with_no_domain_yields_the_digest_alone() {
    // The pasted-password case. Nothing recognisable may survive, including the
    // `@` that would hint at the shape of what was typed.
    for raw in ["hunter2", "not-an-email", "@example.com", "alice@", ""] {
        let masked = email_log_key(raw);
        assert!(
            !masked.contains('@'),
            "{raw:?} → {masked} must reduce to a bare digest"
        );
        assert!(!masked.contains(raw) || raw.is_empty());
    }
}

#[test]
fn the_same_address_maps_to_the_same_key_after_normalisation() {
    // Correlation is what the log line is for: repeated probes of one address
    // must be recognisable as repeats, including across case and whitespace,
    // which is how the address reaches the use case anyway.
    let a = email_log_key("alice@example.com");
    assert_eq!(a, email_log_key("ALICE@Example.com"));
    assert_eq!(a, email_log_key("  alice@example.com  "));
}

#[test]
fn distinct_addresses_map_to_distinct_keys() {
    // Otherwise one prober hitting many addresses is indistinguishable from many
    // attempts against one, which inverts the conclusion drawn from the log.
    let a = email_log_key("alice@example.com");
    let b = email_log_key("bob@example.com");
    let c = email_log_key("alice@other.com");
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(b, c);
}

#[test]
fn the_digest_is_short_enough_to_read_and_long_enough_to_separate() {
    // 12 hex characters. Asserted because shortening it later to "tidy up" the
    // log would start colliding distinct addresses, and the symptom — two
    // attackers appearing as one — reads as normal traffic.
    let masked = email_log_key("alice@example.com");
    let digest = masked.split('@').next().unwrap();
    assert_eq!(digest.len(), 12);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
}
