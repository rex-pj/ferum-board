use axum::http::HeaderMap;
use ferum_web::middleware::rate_limit::extract_client_ip;

// ─── trusted_proxy_count = 0 ──────────────────────────────────────────────────

#[test]
fn no_headers_trusted_zero_returns_unknown() {
    let headers = HeaderMap::new();
    assert_eq!(extract_client_ip(&headers, 0), "unknown");
}

#[test]
fn xff_present_but_trusted_zero_ignores_it() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 0), "unknown");
}

#[test]
fn x_real_ip_present_but_trusted_zero_ignores_it() {
    let mut headers = HeaderMap::new();
    headers.insert("x-real-ip", "203.0.113.5".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 0), "unknown");
}

// ─── trusted_proxy_count = 1 ──────────────────────────────────────────────────

#[test]
fn single_hop_xff_trusted_one_returns_idx_len_minus_n() {
    // Algorithm: idx = parts.len() - n. With 2 parts, trusted=1 → idx=1 → "10.0.0.1".
    // Typical single-proxy deployment sets only 1 IP in XFF (the client), so len=1 idx=0.
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5, 10.0.0.1".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 1), "10.0.0.1");
}

#[test]
fn single_entry_xff_trusted_one_returns_that_entry() {
    // Typical Nginx config: XFF contains exactly 1 IP (the client). len=1, n=1 → idx=0.
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 1), "203.0.113.5");
}

#[test]
fn xff_with_spaces_around_commas_are_trimmed() {
    // Spaces after commas must be stripped; result follows the same idx=len-n rule.
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5 , 10.0.0.1".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 1), "10.0.0.1");
}

// ─── trusted_proxy_count = 2 ──────────────────────────────────────────────────

#[test]
fn two_trusted_proxies_skips_both_rightmost_entries() {
    // Chain: client → proxy1 → proxy2. trusted=2 → len=3, idx=3-2=1 = "proxy1" (client's next hop)
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "203.0.113.1, 10.0.0.1, 10.0.0.2".parse().unwrap(),
    );
    assert_eq!(extract_client_ip(&headers, 2), "10.0.0.1");
}

#[test]
fn two_trusted_proxies_but_only_one_xff_entry_falls_back_to_first() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.1".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 2), "203.0.113.1");
}

// ─── X-Real-IP fallback ───────────────────────────────────────────────────────

#[test]
fn x_real_ip_used_when_xff_absent_and_trusted_positive() {
    let mut headers = HeaderMap::new();
    headers.insert("x-real-ip", "203.0.113.99".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 1), "203.0.113.99");
}

#[test]
fn xff_takes_precedence_over_x_real_ip() {
    // XFF is checked first; X-Real-IP is only a fallback when XFF is absent.
    // With XFF="1.2.3.4, 10.0.0.1" and trusted=1: idx=2-1=1 → "10.0.0.1" (from XFF, not X-Real-IP).
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4, 10.0.0.1".parse().unwrap());
    headers.insert("x-real-ip", "9.9.9.9".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, 1), "10.0.0.1");
}
