//! Locale negotiation and error-message translation.
//!
//! Two middlewares that together make a request locale-aware:
//!
//! * [`negotiate_locale`] runs on the way *in*, resolving the request's locale
//!   and inserting it as a request extension.
//! * [`translate_errors`] runs on the way *out*, rewriting the `message` field
//!   of an error response into that locale.
//!
//! They are separate because they act at opposite ends of the request. The
//! error translator has to be outermost so it sees responses produced by every
//! inner layer — including rejections from the auth and rate-limit middlewares,
//! which never reach a handler.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

use ferum_domain::{ErrorPayload, Locale};

use crate::app_state::AppState;

/// Cookie holding a visitor's chosen language.
///
/// Guests need this to be a cookie rather than `localStorage` (which is how the
/// theme preference works): locale is baked into the server-rendered HTML, so it
/// has to travel *with* the request, not be applied by script afterwards.
pub const LOCALE_COOKIE: &str = "ferum_locale";

/// The locale resolved for this request, plus the path stripped of any locale
/// prefix.
///
/// Page handlers take this as an extension and hand it to `render_with_theme_in`.
/// The canonical path is carried alongside the locale because `hreflang`
/// alternates have to point at *this same page* in every other language, and by
/// the time rendering happens the original URI is long gone.
#[derive(Clone, Debug)]
pub struct RequestLocale {
    pub locale: Locale,
    /// Language-neutral path, always starting with `/`: `/vi/forum/x` → `/forum/x`.
    pub canonical_path: String,
}

impl Default for RequestLocale {
    fn default() -> Self {
        RequestLocale {
            locale: Locale::default_locale(),
            canonical_path: "/".to_string(),
        }
    }
}

/// Resolves the request's locale and attaches it as an extension.
///
/// Precedence, highest first:
///
/// 1. **Path prefix** (`/vi/forum/...`) — an explicit, shareable choice.
/// 2. **`?lang=` query** — the switcher's entry point before the cookie is set.
/// 3. **Cookie** — a returning guest's previous choice.
/// 4. **`Accept-Language`** — the browser's stated preference.
/// 5. **Site default.**
///
/// A signed-in user's stored preference is applied by the auth layer, which runs
/// later and knows who the user is; it overrides 3–5 but not 1–2, so following a
/// `/vi/` link always shows Vietnamese regardless of your account setting.
///
/// Unrecognized values are ignored rather than rejected. A stale cookie naming a
/// locale the admin has since disabled should quietly fall through to the
/// default, not 400 the request and lock the visitor out of the site.
pub async fn negotiate_locale(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let enabled = state.translator.available_locales();

    let from_path = locale_from_path(req.uri().path(), &enabled);
    let locale = from_path
        .clone()
        .or_else(|| locale_from_query(req.uri().query(), &enabled))
        .or_else(|| locale_from_cookie(&req, &enabled))
        .or_else(|| locale_from_accept_language(&req, &enabled))
        .unwrap_or_default();

    // Strip the prefix before the router sees the path, so every route is
    // declared once. Without this each route would need a duplicate `/{locale}/…`
    // arm, and any route added later would silently not work in other languages.
    if from_path.is_some() {
        if let Some(stripped) = strip_locale_prefix(req.uri(), locale.as_str()) {
            *req.uri_mut() = stripped;
        }
    }

    // Captured *after* stripping, so it is the language-neutral path regardless
    // of how the visitor arrived.
    let canonical_path = req.uri().path().to_string();

    // Both are inserted: `translate_errors` only needs the locale, while page
    // handlers need the path too. Keeping the bare `Locale` avoids making the
    // error path depend on a web-layer struct.
    req.extensions_mut().insert(locale.clone());
    req.extensions_mut().insert(RequestLocale {
        locale,
        canonical_path,
    });
    next.run(req).await
}

/// Rebuilds a URI with the leading `/{locale}` segment removed.
///
/// `/vi/forum/t/x?a=1` → `/forum/t/x?a=1`, and a bare `/vi` → `/`.
fn strip_locale_prefix(uri: &axum::http::Uri, tag: &str) -> Option<axum::http::Uri> {
    let path = uri.path();
    let rest = path
        .strip_prefix(&format!("/{tag}"))
        .filter(|r| r.is_empty() || r.starts_with('/'))?;

    let new_path = if rest.is_empty() { "/" } else { rest };
    let new_path_and_query = match uri.query() {
        Some(q) => format!("{new_path}?{q}"),
        None => new_path.to_string(),
    };

    let mut parts = uri.clone().into_parts();
    parts.path_and_query = Some(new_path_and_query.parse().ok()?);
    axum::http::Uri::from_parts(parts).ok()
}

/// Extracts a locale from a leading path segment.
///
/// Only returns a locale that is actually installed, so an unknown `/xx/` prefix
/// falls through to the router and 404s as a normal unmatched path rather than
/// being silently swallowed.
fn locale_from_path(path: &str, enabled: &[Locale]) -> Option<Locale> {
    let first = path.strip_prefix('/')?.split('/').next()?;
    let candidate = Locale::parse(first)?;
    enabled.contains(&candidate).then_some(candidate)
}

fn locale_from_query(query: Option<&str>, enabled: &[Locale]) -> Option<Locale> {
    let query = query?;
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "lang")
        .and_then(|(_, v)| Locale::parse(v))
        .filter(|l| enabled.contains(l))
}

fn locale_from_cookie(req: &Request, enabled: &[Locale]) -> Option<Locale> {
    let raw = req.headers().get(header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == LOCALE_COOKIE)
        .and_then(|(_, v)| Locale::parse(v))
        .filter(|l| enabled.contains(l))
}

/// Picks the best installed locale from an `Accept-Language` header.
///
/// Implements q-value ordering, and falls back from a regional tag to its base
/// language (`vi-VN` matches an installed `vi`) so a browser configured for a
/// specific region still gets a translated page.
fn locale_from_accept_language(req: &Request, enabled: &[Locale]) -> Option<Locale> {
    let raw = req.headers().get(header::ACCEPT_LANGUAGE)?.to_str().ok()?;

    let mut candidates: Vec<(f32, &str)> = raw
        .split(',')
        .filter_map(|part| {
            let mut bits = part.split(';');
            let tag = bits.next()?.trim();
            if tag.is_empty() || tag == "*" {
                return None;
            }
            // `;q=0.8` — anything unparseable is treated as the default weight
            // of 1.0, matching how browsers behave with malformed headers.
            let q = bits
                .find_map(|b| b.trim().strip_prefix("q=").and_then(|v| v.parse().ok()))
                .unwrap_or(1.0);
            Some((q, tag))
        })
        .collect();

    // Descending quality. `sort_by` with `partial_cmp` is safe here because q
    // values that fail to parse never enter the list.
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (_, tag) in candidates {
        let Some(parsed) = Locale::parse(tag) else {
            continue;
        };
        // Exact match first, then narrow toward the base language: en-CA → en.
        //
        // `base_chain`, not `fallback_chain`: the latter ends at the default
        // locale, which would make *every* requested language "match" and hand
        // a German speaker English while claiming it was negotiated.
        for step in parsed.base_chain() {
            if enabled.contains(&step) {
                return Some(step);
            }
        }
    }
    None
}

/// Rewrites the `message` of an error response into the request's locale.
///
/// `AppError::into_response` cannot do this itself: `IntoResponse` sees neither
/// `AppState` nor request extensions, so the only alternatives would be a
/// process-global translator or a task-local — both of which this codebase
/// avoids. Instead the error attaches an [`ErrorPayload`] to the response and
/// this layer, which has both the locale and the translator, resolves it.
///
/// Responses without an `ErrorPayload` — every success, plus free-form
/// validator output — pass through untouched and unbuffered.
pub async fn translate_errors(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // Read the locale before the request is consumed; `negotiate_locale` runs
    // inside this layer, so fall back to the default if it has not run.
    let locale = req
        .extensions()
        .get::<Locale>()
        .cloned()
        .unwrap_or_default();

    let response = next.run(req).await;

    let Some(payload) = response.extensions().get::<ErrorPayload>().cloned() else {
        return response;
    };

    let args = payload.translator_args();
    let message = state
        .translator
        .translate(&locale, &payload.key(), args.as_slice());

    let (mut parts, body) = response.into_parts();

    // Error bodies are small and already fully built, so collecting is cheap.
    // If it fails, keep the original response rather than turning a handled
    // error into a broken one.
    let Ok(bytes) = axum::body::to_bytes(body, 64 * 1024).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };

    if let Some(slot) = json.get_mut("error").and_then(|e| e.get_mut("message")) {
        *slot = serde_json::Value::String(message);
    }

    let rendered = serde_json::to_vec(&json).unwrap_or_else(|_| bytes.to_vec());
    // Length changes with the translation; a stale Content-Length truncates the
    // body or hangs the client.
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(rendered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

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
}
