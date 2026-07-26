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

// ─── Auth cookie attributes ──────────────────────────────────────────────────
// `SameSite=Lax` is load-bearing CSRF protection: there is no CSRF token in this
// codebase, so if the cookie is ever relaxed to `SameSite=None` the Origin check
// in middleware/csrf.rs becomes the only defence and a real token is required.
// These tests exist to make that change loud instead of silent.

#[test]
fn auth_cookie_is_httponly_and_samesite_lax() {
    let c = ferum_web::utils::build_auth_cookie("token", "abc", "/", 3600, false);
    assert!(c.contains("HttpOnly"), "auth cookie must stay HttpOnly: {c}");
    assert!(c.contains("SameSite=Lax"), "auth cookie must stay SameSite=Lax: {c}");
    assert!(!c.contains("SameSite=None"), "SameSite=None removes CSRF protection: {c}");
}

#[test]
fn auth_cookie_sets_secure_only_over_https() {
    let insecure = ferum_web::utils::build_auth_cookie("token", "abc", "/", 3600, false);
    assert!(!insecure.contains("Secure"), "plain HTTP dev must not set Secure: {insecure}");

    let secure = ferum_web::utils::build_auth_cookie("token", "abc", "/", 3600, true);
    assert!(secure.contains("; Secure"), "HTTPS deploys must set Secure: {secure}");
}

#[test]
fn refresh_cookie_is_scoped_to_auth_endpoints() {
    // Path scoping keeps the refresh token off every ordinary request, so it is
    // only ever sent where it is actually redeemed.
    let c = ferum_web::utils::build_auth_cookie("refresh_token", "r", "/api/auth", 604_800, true);
    assert!(c.contains("Path=/api/auth"), "refresh cookie must stay scoped: {c}");
}
