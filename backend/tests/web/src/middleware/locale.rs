use axum::body::Body;
use axum::extract::Request;
use axum::http::header;
use axum::http::Request as HttpRequest;

use ferum_domain::Locale;
use ferum_web::middleware::locale::{
    canonical_query, locale_from_accept_language, locale_from_cookie, locale_from_path,
    locale_from_query, site_default_locale, strip_locale_prefix,
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

#[test]
fn the_site_default_is_the_last_fallback_and_must_be_installed() {
    // The setting exists so a Vietnamese forum can serve Vietnamese at `/`. It went
    // unread for its whole life — `negotiate_locale` used `Locale::default()` — so
    // "set as default" on /admin/languages changed nothing a visitor could see.
    let installed = enabled();
    assert_eq!(site_default_locale(Some("vi"), &installed), Locale::parse("vi").unwrap());

    // Not installed → the source locale, never the configured tag. Honouring it
    // would serve a language with no catalog, rendering every string as a raw key.
    assert_eq!(site_default_locale(Some("de"), &installed), Locale::default_locale());
    // Unset, blank and unparseable all mean "use the source locale".
    for raw in [None, Some(""), Some("not-a-locale"), Some("../etc/passwd")] {
        assert_eq!(
            site_default_locale(raw, &installed),
            Locale::default_locale(),
            "{raw:?} must fall back to the source locale"
        );
    }
}

#[test]
fn a_non_canonical_default_still_resolves() {
    // `Locale::parse` canonicalises, so a hand-edited row saying `VI` names the same
    // language as `vi` and must not read as "not installed".
    assert_eq!(
        site_default_locale(Some("VI"), &enabled()),
        Locale::parse("vi").unwrap()
    );
}

#[test]
fn the_site_default_does_not_override_an_expressed_preference() {
    // The site default is the LAST link in the precedence chain, so every explicit
    // signal still wins. Pinned here on the readers themselves: a change that made
    // the default outrank a cookie would silently ignore what a visitor chose.
    let installed = enabled();
    let vi = Locale::parse("vi").unwrap();
    assert_eq!(locale_from_path("/ja/forum", &installed), Locale::parse("ja"));
    assert_eq!(locale_from_query(Some("lang=ja"), &installed), Locale::parse("ja"));
    assert_eq!(
        locale_from_cookie(&req_with(header::COOKIE, "ferum_locale=ja"), &installed),
        Locale::parse("ja")
    );
    // …and none of them is affected by what the site default happens to be.
    assert_eq!(site_default_locale(Some("vi"), &installed), vi);
}

#[test]
fn a_canonical_url_keeps_pagination_and_drops_everything_else() {
    // `rel=canonical` exists to collapse the several URLs that reach one page, so the
    // query has to be filtered rather than kept or dropped wholesale. Keeping it lets
    // `?utm_source=x` mint a fresh canonical per campaign; dropping it points page 3
    // of a listing at page 1, which takes every thread that only appears later out of
    // the index.
    assert_eq!(canonical_query(Some("page=3")), Some("?page=3".to_string()));
    assert_eq!(
        canonical_query(Some("sort=hot&page=12&utm_source=x")),
        Some("?page=12".to_string())
    );
    // A repeated key resolves the way `serde_urlencoded` resolves it for the handler:
    // last wins. Otherwise the canonical could name a different page than the one
    // actually rendered.
    assert_eq!(
        canonical_query(Some("page=2&page=3")),
        Some("?page=3".to_string())
    );

    for dropped in [
        None,
        // Page 1 and a bare path are the same page; self-canonicalising both spellings
        // would leave the duplicate standing.
        Some("page=1"),
        Some("page=0"),
        Some("utm_source=newsletter&utm_medium=email"),
        // A sort or filter reorders the same items — a duplicate, not a page of its own.
        Some("sort=hot&tag=rust"),
        // The locale is carried by the URL prefix; a `?lang=` copy is the duplicate.
        Some("lang=vi"),
        // Unparseable rather than absent: fall back to the bare path, never emit
        // `?page=` with a value a crawler cannot follow.
        Some("page=abc"),
        Some("page="),
        Some("page"),
        Some(""),
    ] {
        assert_eq!(
            canonical_query(dropped),
            None,
            "{dropped:?} must not reach the canonical URL"
        );
    }
}
