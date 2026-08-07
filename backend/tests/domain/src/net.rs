use std::net::IpAddr;

use ferum_domain::net::{is_private_host_literal, is_private_ip, normalize_host};

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn private_ipv4_ranges_are_rejected() {
    for s in ["127.0.0.1", "10.0.0.1", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "0.0.0.0"] {
        assert!(is_private_ip(ip(s)), "{s} must be treated as private");
    }
}

#[test]
fn public_ipv4_is_allowed() {
    for s in ["8.8.8.8", "1.1.1.1", "172.32.0.1", "100.63.255.255"] {
        assert!(!is_private_ip(ip(s)), "{s} must be treated as public");
    }
}

#[test]
fn private_ipv6_ranges_are_rejected() {
    for s in ["::1", "::", "fe80::1", "fc00::1", "fd00::1", "::ffff:127.0.0.1"] {
        assert!(is_private_ip(ip(s)), "{s} must be treated as private");
    }
}

#[test]
fn public_ipv6_is_allowed() {
    assert!(!is_private_ip(ip("2606:4700::1111")));
}

/// `http::Uri::host()` yields IPv6 literals wrapped in brackets. Forgetting
/// to strip them makes every `http://[::1]/` URL look like a public hostname.
#[test]
fn bracketed_ipv6_literals_are_recognised_as_private() {
    assert_eq!(normalize_host("[::1]"), "::1");
    assert_eq!(normalize_host("127.0.0.1"), "127.0.0.1");
    assert_eq!(normalize_host("example.com"), "example.com");

    assert!(is_private_host_literal("[::1]"));
    assert!(is_private_host_literal("[fd00::1]"));
    assert!(is_private_host_literal("[fe80::1]"));
}

#[test]
fn localhost_and_ip_literals_are_private_hosts() {
    assert!(is_private_host_literal("localhost"));
    assert!(is_private_host_literal("LOCALHOST"));
    assert!(is_private_host_literal("127.0.0.1"));
    assert!(is_private_host_literal("169.254.169.254"));
}

/// A bare hostname cannot be judged without DNS; that is the caller's job.
#[test]
fn plain_hostname_is_not_private_by_literal_inspection() {
    assert!(!is_private_host_literal("example.com"));
    assert!(!is_private_host_literal("hooks.slack.com"));
}
