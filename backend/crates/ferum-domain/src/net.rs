//! Pure IP-address classification. No I/O, no DNS — just "is this address one
//! we must never let an outbound request reach?".
//!
//! Lives in the domain because two layers need the same answer and must not
//! disagree: `ferum-application` validates admin-entered webhook URLs, and
//! `ferum-infrastructure` validates the addresses it is about to connect to.
//! Two copies of this predicate would be two chances to get SSRF wrong.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// True when `ip` is loopback, private, link-local (which covers the
/// 169.254.169.254 cloud-metadata endpoint), CGNAT, broadcast, or unspecified.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_ipv4(v4),
        IpAddr::V6(v6) => is_private_ipv6(v6),
    }
}

fn is_private_ipv4(v4: Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_unspecified()
        || matches!(v4.octets(), [100, 64..=127, _, _]) // CGNAT 100.64.0.0/10
}

fn is_private_ipv6(v6: Ipv6Addr) -> bool {
    let seg = v6.segments();
    v6.is_loopback()
        || v6.is_unspecified()
        || (seg[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
        || (seg[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        || v6.to_ipv4_mapped().is_some_and(is_private_ipv4)
}

/// Strips the surrounding brackets that a URL keeps around IPv6 literals.
///
/// `http::Uri::host()` returns `"[::1]"`, not `"::1"`, and `"[::1]"` does not
/// parse as an `IpAddr` — so any check that forgets this silently waves through
/// every IPv6 loopback/ULA literal.
pub fn normalize_host(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
}

/// True when `host` — a URL host component, which may be a bare hostname, an
/// IPv4 literal, or a bracketed IPv6 literal — is *known* to be private from its
/// text alone. A hostname that merely resolves to a private address returns
/// `false` here; only DNS can catch that, which is I/O and thus the caller's job.
pub fn is_private_host_literal(host: &str) -> bool {
    let host = normalize_host(host);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) => is_private_ip(ip),
        Err(_) => false,
    }
}
