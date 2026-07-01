/// Resolves the hostname in `url` via DNS and rejects any address that falls
/// in a private, loopback, link-local, or CGNAT range (DNS re-binding defence).
pub async fn assert_no_private_ip(url: &str) -> Result<(), String> {
    resolve_and_validate(url).await.map(|_| ())
}

/// Resolves the hostname in `url`, validates every candidate address is public,
/// and returns the validated addresses so the caller can pin the connection to
/// them — re-resolving DNS for the actual request would reopen the TOCTOU window
/// this check exists to close (DNS re-binding between validation and connect).
pub async fn resolve_and_validate(url: &str) -> Result<Vec<std::net::SocketAddr>, String> {
    let parsed = url::Url::parse(url)
        .map_err(|_| format!("unparseable URL: {}", url))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let port = parsed
        .port()
        .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });

    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(format!("{}:{}", host, port))
        .await
        .map_err(|e| format!("DNS resolution failed for {}: {}", host, e))?
        .collect();

    for socket_addr in &addrs {
        let ip = socket_addr.ip();
        if is_private_ip(ip) {
            return Err(format!(
                "resolved IP {} is in a private/reserved range",
                ip
            ));
        }
    }
    Ok(addrs)
}

pub fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || matches!(v4.octets(), [100, 64..=127, _, _]) // CGNAT 100.64.0.0/10
        }
        std::net::IpAddr::V6(v6) => {
            let seg = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || (seg[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (seg[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || v6.to_ipv4_mapped().map_or(false, |v4| {
                    v4.is_loopback()
                        || v4.is_private()
                        || v4.is_link_local()
                        || v4.is_unspecified()
                })
        }
    }
}
