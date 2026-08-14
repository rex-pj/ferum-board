//! How the SMTP connection is secured.
//!
//! `AsyncSmtpTransport` exposes no getter for the TLS mode it was built with,
//! which is why `security_for` exists as a named function rather than inline at
//! the call site: it is the only way the decision can be observed at all.

use ferum_infrastructure::email::{
    security_for, validate_from_address, LettreEmailService, SmtpSecurity,
};

#[test]
fn loopback_hosts_connect_in_the_clear() {
    // A dev mail catcher presents no certificate. Every spelling of "this machine"
    // has to be recognised, because missing one turns a working dev setup into a
    // TLS handshake against something that does not speak TLS.
    for host in [
        "localhost",
        "LOCALHOST",
        "LocalHost",
        "127.0.0.1",
        "127.1.2.3", // the whole 127.0.0.0/8 block, not just .1
        "::1",
        "[::1]",     // the bracketed form a URL would carry
        " localhost ",
    ] {
        assert_eq!(
            security_for(host, 1025),
            SmtpSecurity::Plaintext,
            "{host} should be treated as loopback"
        );
    }
}

#[test]
fn port_465_uses_implicit_tls() {
    // SMTPS speaks TLS from the first byte, so STARTTLS — which negotiates inside
    // an already-open plaintext session — can never complete there. Getting this
    // wrong is a hang or a handshake failure, not a fallback.
    assert_eq!(
        security_for("smtp.resend.com", 465),
        SmtpSecurity::ImplicitTls
    );
    assert_eq!(security_for("smtp.gmail.com", 465), SmtpSecurity::ImplicitTls);
}

#[test]
fn every_other_host_requires_starttls() {
    for (host, port) in [
        ("smtp.sendgrid.net", 587),
        ("smtp.resend.com", 587),
        ("smtp.resend.com", 2587),
        ("email-smtp.eu-west-1.amazonaws.com", 25),
    ] {
        assert_eq!(
            security_for(host, port),
            SmtpSecurity::StartTls,
            "{host}:{port} should require STARTTLS"
        );
    }
}

#[test]
fn a_private_lan_relay_is_not_treated_as_loopback() {
    // Deliberately narrower than `ferum_domain::net::is_private_ip`. Exempting the
    // private ranges would let credentials cross a LAN in the clear, and "the
    // packet stays inside our network" is exactly the assumption that makes a flat
    // internal network a credential-harvesting opportunity.
    for host in ["10.0.0.5", "192.168.1.20", "172.16.0.9", "mail.internal"] {
        assert_eq!(
            security_for(host, 587),
            SmtpSecurity::StartTls,
            "{host} must not be exempted from TLS"
        );
    }
}

#[test]
fn port_25_on_a_remote_host_still_requires_tls() {
    // Port-based inference would make 25 plaintext by convention. That is the case
    // this rule exists to refuse: it is the port most likely to be reached over the
    // open internet.
    assert_eq!(security_for("mx.example.com", 25), SmtpSecurity::StartTls);
    // …but on loopback it stays plaintext, because the host rule wins.
    assert_eq!(security_for("127.0.0.1", 25), SmtpSecurity::Plaintext);
}

#[test]
fn a_loopback_transport_builds() {
    let svc = LettreEmailService::new("localhost", 1025, None, None, "noreply@example.com");
    let svc = svc.expect("a loopback transport should build");
    let described = svc.describe();
    assert!(described.contains("localhost:1025"));
    // The label must say which mode is in force — "email enabled" alone does not
    // tell an operator whether credentials are protected.
    assert!(described.contains("no TLS"));
}

#[test]
fn a_tls_transport_builds_and_names_its_mode() {
    let starttls =
        LettreEmailService::new("smtp.example.com", 587, Some("u"), Some("p"), "n@example.com")
            .expect("STARTTLS transport should build");
    assert!(starttls.describe().contains("STARTTLS"));

    let implicit =
        LettreEmailService::new("smtp.example.com", 465, Some("u"), Some("p"), "n@example.com")
            .expect("implicit TLS transport should build");
    assert!(implicit.describe().contains("implicit TLS"));
}

#[test]
fn a_malformed_hostname_still_builds_because_tls_validates_at_connect_time() {
    // Worth pinning, because it is the opposite of what the API shape suggests.
    // `starttls_relay` and `relay` return `Result`, so it reads as though a bad
    // host is rejected here. It is not: `TlsParameters::new` only stores the
    // domain, and rustls does not parse it as a server name until the connection
    // is made. The only build-time failure is loading the root certificate store.
    //
    // The consequence for the rest of this module: `reload`'s error arm is still
    // effectively unreachable, so `sync_auto_verify` on that path is defensive
    // rather than a fix for a live bug. See the note in reloadable_service.rs.
    let built = LettreEmailService::new(
        "not a valid host name",
        587,
        None,
        None,
        "noreply@example.com",
    );
    assert!(
        built.is_ok(),
        "lettre defers hostname validation to connect time; if this now fails, \
         the error path in ReloadableEmailService::reload has become reachable and \
         the auto_verify tests should force it"
    );
}

// ─── FROM_EMAIL ──────────────────────────────────────────────────────────────

#[test]
fn a_bare_address_is_a_valid_sender() {
    for from in [
        "noreply@example.com",
        "no-reply@sub.example.co.uk",
        "forum+notifications@example.com",
    ] {
        assert!(
            validate_from_address(from).is_ok(),
            "{from:?} should be accepted"
        );
    }
}

#[test]
fn a_display_name_sender_is_valid() {
    // The reason there is no separate `EMAIL_FROM_NAME`: the name goes inside
    // FROM_EMAIL in the RFC 5322 form, and both adapters take it — lettre parses it
    // into a Mailbox, and Resend's API documents the same shape for its `from`
    // field. This test is what makes that a checked claim rather than a reading of
    // lettre's source.
    for from in [
        "Ferum Board <noreply@example.com>",
        "Qhortus <no-reply@qhortus.com>",
        "<noreply@example.com>",
    ] {
        assert!(
            validate_from_address(from).is_ok(),
            "{from:?} should be accepted"
        );
    }
}

#[test]
fn a_malformed_sender_is_rejected() {
    // Every one of these used to start the process cleanly and then fail every
    // message with a generic `internal_error`.
    for from in [
        "",
        "   ",
        "noreply",              // no domain
        "noreply@",             // no domain part
        "@example.com",         // no local part
        "noreply at example.com",
        "Ferum Board <noreply@example.com",  // unclosed bracket
        "Ferum Board noreply@example.com",   // name without brackets
    ] {
        assert!(
            validate_from_address(from).is_err(),
            "{from:?} should be rejected"
        );
    }
}

#[test]
fn the_rejection_message_says_what_a_valid_value_looks_like() {
    // It goes into a startup abort, so it has to be actionable on its own — the
    // operator is looking at a container that will not come up.
    let Err(err) = validate_from_address("noreply") else {
        panic!("expected rejection");
    };
    let msg = err.to_string();
    assert!(msg.contains("FROM_EMAIL"));
    assert!(msg.contains("Display Name"));
}

#[test]
fn a_display_name_sender_also_builds_a_transport() {
    // Guards the whole path, not just the validator: `send` parses `from` through
    // the same function, so a form accepted at boot must not be refused later.
    let svc = LettreEmailService::new(
        "localhost",
        1025,
        None,
        None,
        "Ferum Board <noreply@example.com>",
    );
    assert!(svc.is_ok());
}

#[test]
fn describe_never_echoes_the_password() {
    // `describe()` goes straight into the startup log.
    let svc = LettreEmailService::new(
        "smtp.example.com",
        587,
        Some("apikey"),
        Some("SUPER-SECRET-VALUE"),
        "noreply@example.com",
    )
    .expect("transport should build");
    let described = svc.describe();
    assert!(!described.contains("SUPER-SECRET-VALUE"));
    assert!(!described.contains("apikey"));
}
