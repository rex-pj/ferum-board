use axum::http::HeaderMap;
use ferum_web::utils::guest_fingerprint;

// ─── Returns None when both IP and UA are empty ───────────────────────────────

#[test]
fn empty_headers_returns_none() {
    let headers = HeaderMap::new();
    assert!(guest_fingerprint(&headers).is_none());
}

// ─── Returns Some(16-char hex) when IP or UA is present ──────────────────────

#[test]
fn xff_only_returns_some_hex_string() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    let fp = guest_fingerprint(&headers).expect("should return Some");
    assert_eq!(fp.len(), 16);
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn x_real_ip_only_returns_some() {
    let mut headers = HeaderMap::new();
    headers.insert("x-real-ip", "203.0.113.5".parse().unwrap());
    let fp = guest_fingerprint(&headers).expect("should return Some");
    assert_eq!(fp.len(), 16);
}

#[test]
fn user_agent_only_returns_some() {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "Mozilla/5.0".parse().unwrap());
    let fp = guest_fingerprint(&headers).expect("should return Some");
    assert_eq!(fp.len(), 16);
}

// ─── Determinism: same inputs produce same fingerprint ───────────────────────

#[test]
fn same_ip_and_ua_produce_same_fingerprint() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    headers.insert("user-agent", "Mozilla/5.0".parse().unwrap());
    let fp1 = guest_fingerprint(&headers).unwrap();
    let fp2 = guest_fingerprint(&headers).unwrap();
    assert_eq!(fp1, fp2);
}

// ─── Different inputs produce different fingerprints ─────────────────────────

#[test]
fn different_ua_on_same_ip_produces_different_fingerprint() {
    let mut h1 = HeaderMap::new();
    h1.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    h1.insert("user-agent", "Mozilla/5.0 Chrome".parse().unwrap());

    let mut h2 = HeaderMap::new();
    h2.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    h2.insert("user-agent", "Mozilla/5.0 Firefox".parse().unwrap());

    assert_ne!(guest_fingerprint(&h1), guest_fingerprint(&h2));
}

#[test]
fn different_ip_on_same_ua_produces_different_fingerprint() {
    let mut h1 = HeaderMap::new();
    h1.insert("x-forwarded-for", "203.0.113.1".parse().unwrap());
    h1.insert("user-agent", "Mozilla/5.0".parse().unwrap());

    let mut h2 = HeaderMap::new();
    h2.insert("x-forwarded-for", "203.0.113.2".parse().unwrap());
    h2.insert("user-agent", "Mozilla/5.0".parse().unwrap());

    assert_ne!(guest_fingerprint(&h1), guest_fingerprint(&h2));
}

// ─── XFF: only first (leftmost) IP is used ───────────────────────────────────

#[test]
fn xff_multi_hop_uses_first_ip() {
    let mut h1 = HeaderMap::new();
    h1.insert("x-forwarded-for", "203.0.113.5, 10.0.0.1".parse().unwrap());

    let mut h2 = HeaderMap::new();
    h2.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());

    assert_eq!(guest_fingerprint(&h1), guest_fingerprint(&h2));
}
