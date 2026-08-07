use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ferum_infrastructure::network_utils::is_private_ip;

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

// Mirrors `Ipv6Addr::new`, which takes eight groups because an IPv6 address
// has eight groups.
#[allow(clippy::too_many_arguments)]
fn v6(a: u16, b: u16, c: u16, d: u16, e: u16, f: u16, g: u16, h: u16) -> IpAddr {
    IpAddr::V6(Ipv6Addr::new(a, b, c, d, e, f, g, h))
}

// ─── IPv4 ─────────────────────────────────────────────────────────────────────

#[test]
fn loopback_127_is_private() {
    assert!(is_private_ip(v4(127, 0, 0, 1)));
    assert!(is_private_ip(v4(127, 255, 255, 255)));
}

#[test]
fn rfc1918_10_block_is_private() {
    assert!(is_private_ip(v4(10, 0, 0, 1)));
    assert!(is_private_ip(v4(10, 255, 255, 255)));
}

#[test]
fn rfc1918_172_block_is_private() {
    assert!(is_private_ip(v4(172, 16, 0, 1)));
    assert!(is_private_ip(v4(172, 31, 255, 255)));
    assert!(!is_private_ip(v4(172, 15, 0, 1)));
    assert!(!is_private_ip(v4(172, 32, 0, 1)));
}

#[test]
fn rfc1918_192_168_block_is_private() {
    assert!(is_private_ip(v4(192, 168, 0, 1)));
    assert!(is_private_ip(v4(192, 168, 100, 200)));
}

#[test]
fn cgnat_100_64_block_is_private() {
    assert!(is_private_ip(v4(100, 64, 0, 1)));
    assert!(is_private_ip(v4(100, 127, 255, 255)));
    assert!(!is_private_ip(v4(100, 63, 255, 255)));
    assert!(!is_private_ip(v4(100, 128, 0, 1)));
}

#[test]
fn link_local_169_254_is_private() {
    assert!(is_private_ip(v4(169, 254, 0, 1)));
    assert!(is_private_ip(v4(169, 254, 255, 255)));
}

#[test]
fn broadcast_is_private() {
    assert!(is_private_ip(v4(255, 255, 255, 255)));
}

#[test]
fn unspecified_0_0_0_0_is_private() {
    assert!(is_private_ip(v4(0, 0, 0, 0)));
}

#[test]
fn public_ipv4_addresses_are_not_private() {
    assert!(!is_private_ip(v4(8, 8, 8, 8)));
    assert!(!is_private_ip(v4(1, 1, 1, 1)));
    assert!(!is_private_ip(v4(93, 184, 216, 34)));
    assert!(!is_private_ip(v4(11, 0, 0, 1)));
}

// ─── IPv6 ─────────────────────────────────────────────────────────────────────

#[test]
fn ipv6_loopback_is_private() {
    assert!(is_private_ip(v6(0, 0, 0, 0, 0, 0, 0, 1)));
}

#[test]
fn ipv6_unspecified_is_private() {
    assert!(is_private_ip(v6(0, 0, 0, 0, 0, 0, 0, 0)));
}

#[test]
fn ipv6_link_local_fe80_is_private() {
    assert!(is_private_ip(v6(0xfe80, 0, 0, 0, 0, 0, 0, 1)));
}

#[test]
fn ipv6_unique_local_fc_is_private() {
    assert!(is_private_ip(v6(0xfc00, 0, 0, 0, 0, 0, 0, 1)));
}

#[test]
fn ipv6_unique_local_fd_is_private() {
    assert!(is_private_ip(v6(0xfd00, 0, 0, 0, 0, 0, 0, 1)));
}

#[test]
fn public_ipv6_is_not_private() {
    // 2606:4700::1111 — Cloudflare DNS
    assert!(!is_private_ip(v6(0x2606, 0x4700, 0, 0, 0, 0, 0, 0x1111)));
}
