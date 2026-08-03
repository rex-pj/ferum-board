use axum::http::HeaderMap;
use ferum_application::constants::MAX_PAGE;
use ferum_web::utils::{etag_for, guest_fingerprint, if_none_match_hits, paginate};

// ─── CAS ETag revalidation ────────────────────────────────────────────────────
//
// `/files/` answers a matching `If-None-Match` with 304 before it queries the
// database, which is sound only because a CAS key is a digest of its own
// content. These pin the matching rules a browser actually exercises.

fn inm(value: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(axum::http::header::IF_NONE_MATCH, value.parse().unwrap());
    h
}

#[test]
fn etag_is_the_quoted_key() {
    assert_eq!(etag_for("sha256-abc"), "\"sha256-abc\"");
}

#[test]
fn absent_if_none_match_is_a_miss() {
    assert!(!if_none_match_hits(&HeaderMap::new(), "sha256-abc"));
}

#[test]
fn exact_etag_matches() {
    assert!(if_none_match_hits(&inm("\"sha256-abc\""), "sha256-abc"));
}

#[test]
fn a_different_key_does_not_match() {
    assert!(!if_none_match_hits(&inm("\"sha256-abc\""), "sha256-xyz"));
}

#[test]
fn weak_validator_matches() {
    // Caches and proxies legitimately weaken validators; treating `W/"x"` as a
    // miss would silently disable the short-circuit for those clients.
    assert!(if_none_match_hits(&inm("W/\"sha256-abc\""), "sha256-abc"));
}

#[test]
fn star_matches_anything() {
    assert!(if_none_match_hits(&inm("*"), "sha256-anything"));
}

#[test]
fn matches_within_a_list_of_candidates() {
    let headers = inm("\"sha256-other\", W/\"sha256-abc\", \"sha256-third\"");
    assert!(if_none_match_hits(&headers, "sha256-abc"));
}

#[test]
fn an_unquoted_key_does_not_match() {
    // Guards against a caller passing the bare key as an ETag: quoting is what
    // makes the comparison unambiguous.
    assert!(!if_none_match_hits(&inm("sha256-abc"), "sha256-abc"));
}

// ─── paginate ─────────────────────────────────────────────────────────────────
//
// The `page` ceiling is a load control, not a UX preference: every list in this
// codebase paginates with LIMIT/OFFSET, so an unbounded `?page=` lets an
// anonymous request make Postgres walk and discard millions of rows.

#[test]
fn paginate_applies_defaults_when_query_is_absent() {
    let (page, per_page) = paginate(None, None, 20, 100).expect("defaults are in range");
    assert_eq!((page, per_page), (1, 20));
}

#[test]
fn paginate_clamps_per_page_to_the_endpoint_maximum() {
    let (_, per_page) = paginate(Some(1), Some(10_000), 20, 100).expect("page 1 is in range");
    assert_eq!(per_page, 100, "per_page is clamped, not rejected");
}

#[test]
fn paginate_floors_page_and_per_page_at_one() {
    // `page = 0` would underflow the repositories' `(page - 1) * per_page`,
    // and `per_page = 0` is a LIMIT of nothing.
    let (page, per_page) = paginate(Some(0), Some(0), 20, 100).expect("zero floors, not errors");
    assert_eq!((page, per_page), (1, 1));
}

#[test]
fn paginate_accepts_the_last_page_on_the_boundary() {
    let (page, _) = paginate(Some(MAX_PAGE), None, 20, 100).expect("MAX_PAGE itself is reachable");
    assert_eq!(page, MAX_PAGE);
}

#[test]
fn paginate_rejects_a_page_past_the_ceiling() {
    let err = paginate(Some(MAX_PAGE + 1), None, 20, 100)
        .expect_err("one past the ceiling must not be served");
    let (status, code) = err.status_and_code();
    assert_eq!(code, "page_out_of_range");
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn paginate_rejects_rather_than_silently_clamping() {
    // The distinction that matters: answering `?page=999999` with page 500's
    // contents would report success for a request that was never honoured.
    assert!(paginate(Some(999_999), None, 20, 100).is_err());
}

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
