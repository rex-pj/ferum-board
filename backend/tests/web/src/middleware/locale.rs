use axum::body::Body;
use axum::extract::Request;
use axum::http::header;
use axum::http::Request as HttpRequest;

use ferum_domain::Locale;
use ferum_web::middleware::locale::{
    locale_from_accept_language, locale_from_cookie, locale_from_path, locale_from_query,
    strip_locale_prefix,
};

fn enabled() -> Vec<Locale> {
    vec![
        Locale::default_locale(),
        Locale::parse("vi").unwrap(),
        Locale::parse("ja").unwrap(),
    ]
}

fn req_with(header_name: header::HeaderName, value: &str) -> Request {
    HttpRequest::builder()
        .uri("/")
        .header(header_name, value)
        .body(Body::empty())
        .unwrap()
}

#[test]
fn path_prefix_wins_and_must_be_installed() {
    assert_eq!(
        locale_from_path("/vi/forum/t/x", &enabled()),
        Locale::parse("vi")
    );
    // Valid tag, not installed → no match, so the router 404s normally.
    assert_eq!(locale_from_path("/de/forum", &enabled()), None);
    // Not a locale at all → an ordinary route.
    assert_eq!(locale_from_path("/forum/t/x", &enabled()), None);
    assert_eq!(locale_from_path("/", &enabled()), None);
}

#[test]
fn query_param_is_read() {
    assert_eq!(
        locale_from_query(Some("foo=1&lang=vi&bar=2"), &enabled()),
        Locale::parse("vi")
    );
    assert_eq!(locale_from_query(Some("lang=de"), &enabled()), None);
    assert_eq!(locale_from_query(None, &enabled()), None);
}

#[test]
fn cookie_is_read_among_others() {
    let req = req_with(header::COOKIE, "theme=dark; ferum_locale=vi; other=1");
    assert_eq!(locale_from_cookie(&req, &enabled()), Locale::parse("vi"));
}

#[test]
fn stale_cookie_for_disabled_locale_is_ignored_not_rejected() {
    // The admin disabled `de` after this visitor chose it. They should get
    // the default silently, not an error page.
    let req = req_with(header::COOKIE, "ferum_locale=de");
    assert_eq!(locale_from_cookie(&req, &enabled()), None);
}

#[test]
fn hostile_cookie_value_cannot_escape_validation() {
    let req = req_with(header::COOKIE, "ferum_locale=../../etc/passwd");
    assert_eq!(locale_from_cookie(&req, &enabled()), None);
}

#[test]
fn accept_language_respects_q_ordering() {
    let req = req_with(header::ACCEPT_LANGUAGE, "de;q=0.9, vi;q=0.8, ja;q=1.0");
    assert_eq!(
        locale_from_accept_language(&req, &enabled()),
        Locale::parse("ja")
    );
}

#[test]
fn accept_language_falls_back_from_region_to_base_language() {
    let req = req_with(header::ACCEPT_LANGUAGE, "vi-VN,en;q=0.5");
    assert_eq!(
        locale_from_accept_language(&req, &enabled()),
        Locale::parse("vi")
    );
}

#[test]
fn accept_language_skips_uninstalled_and_wildcards() {
    let req = req_with(header::ACCEPT_LANGUAGE, "*,de,fr;q=0.9,vi;q=0.1");
    assert_eq!(
        locale_from_accept_language(&req, &enabled()),
        Locale::parse("vi")
    );
}

#[test]
fn locale_prefix_is_stripped_before_routing() {
    let uri: axum::http::Uri = "/vi/forum/t/hello".parse().unwrap();
    assert_eq!(
        strip_locale_prefix(&uri, "vi").unwrap().to_string(),
        "/forum/t/hello"
    );
}

#[test]
fn stripping_preserves_the_query_string() {
    let uri: axum::http::Uri = "/vi/search?q=rust&page=2".parse().unwrap();
    assert_eq!(
        strip_locale_prefix(&uri, "vi").unwrap().to_string(),
        "/search?q=rust&page=2"
    );
}

#[test]
fn bare_locale_root_becomes_site_root() {
    let uri: axum::http::Uri = "/vi".parse().unwrap();
    assert_eq!(strip_locale_prefix(&uri, "vi").unwrap().to_string(), "/");

    let uri: axum::http::Uri = "/vi/".parse().unwrap();
    assert_eq!(strip_locale_prefix(&uri, "vi").unwrap().to_string(), "/");
}

#[test]
fn stripping_does_not_match_a_longer_first_segment() {
    // `/vietnam` must not be read as `/vi` + `etnam`, which would route the
    // request to `/etnam` and 404 a perfectly good page.
    let uri: axum::http::Uri = "/vietnam/forum".parse().unwrap();
    assert!(strip_locale_prefix(&uri, "vi").is_none());
}

#[test]
fn uninstalled_language_does_not_silently_match_the_default() {
    // Regression: an early version matched via `fallback_chain`, which ends
    // at the default locale — so a German-only browser "negotiated" to
    // English and every other preference in the header was ignored.
    let req = req_with(header::ACCEPT_LANGUAGE, "de");
    assert_eq!(locale_from_accept_language(&req, &enabled()), None);

    // …and with a lower-priority installed language present, that one wins
    // rather than the default swallowing the match at `de`.
    let req = req_with(header::ACCEPT_LANGUAGE, "de,vi;q=0.1");
    assert_eq!(
        locale_from_accept_language(&req, &enabled()),
        Locale::parse("vi")
    );
}

#[test]
fn malformed_accept_language_does_not_panic() {
    for raw in ["", ";;;", "q=", "en;q=notanumber", "????"] {
        let req = req_with(header::ACCEPT_LANGUAGE, raw);
        let _ = locale_from_accept_language(&req, &enabled());
    }
}
