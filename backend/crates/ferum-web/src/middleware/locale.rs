//! Locale negotiation and error-message translation.
//!
//! [`negotiate_locale`] resolves the locale on the way in; [`translate_errors`]
//! rewrites error messages on the way out. Separate because the translator must
//! be OUTERMOST to catch rejections from auth and rate-limit, which never reach
//! a handler.

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

/// `site_config` key naming the locale served to a visitor with no preference.
///
/// Absent means [`Locale::default_locale`] — the source locale. Deliberately not
/// seeded, because "unset" and "explicitly English" want to stay distinguishable.
pub const DEFAULT_LOCALE_KEY: &str = "default_locale";

/// The locale resolved for this request, plus the path stripped of any locale
/// prefix.
///
/// Page handlers take this as an extension and hand it to `render_with_theme_in`.
/// The canonical path is carried alongside the locale because `hreflang` alternates
/// and `rel=canonical` both have to point at *this same page*, and by the time
/// rendering happens the original URI is long gone.
#[derive(Clone, Debug)]
pub struct RequestLocale {
    pub locale: Locale,
    /// Language-neutral path, always starting with `/`: `/vi/forum/x` → `/forum/x`.
    ///
    /// Carries the pagination query when there is one (`/forum?page=3`) and nothing
    /// else — see [`canonical_query`].
    pub canonical_path: String,
    /// The locale served on **unprefixed** URLs, from `site_config.default_locale`.
    ///
    /// Carried per request rather than looked up at render time so `hreflang` can
    /// tell which alternate is the unprefixed one without a second config read —
    /// and so the answer cannot differ between the negotiation that chose the
    /// locale and the markup that describes it.
    pub site_default: Locale,
}

impl Default for RequestLocale {
    fn default() -> Self {
        RequestLocale {
            locale: Locale::default_locale(),
            canonical_path: "/".to_string(),
            site_default: Locale::default_locale(),
        }
    }
}

/// The part of a query string that belongs in a canonical URL: `?page=N` for N > 1,
/// and nothing else.
///
/// **Dropping the whole query would be worse than emitting none at all.** A canonical
/// of `/forum` on `/forum?page=3` tells a crawler page 3 is a duplicate of page 1, so
/// every thread that appears only on a later page drops out of the index — the exact
/// opposite of what the tag is for.
///
/// Everything else is dropped on purpose. `utm_*` and friends are what `rel=canonical`
/// primarily exists to collapse; `?lang=` names a language the URL prefix already
/// carries; and sort/filter params produce the same set of items in a different order,
/// which is a duplicate. `page=1` is dropped because it and a bare path are the same
/// page, and self-canonicalising both spellings would leave the duplicate standing.
pub fn canonical_query(query: Option<&str>) -> Option<String> {
    let page = query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        // `rfind`, so a repeated key resolves the way `serde_urlencoded` resolves it
        // for the handler's `page` field: last wins. A canonical naming a different
        // page than the one rendered would have that page report itself a duplicate.
        .rfind(|(k, _)| *k == "page")?
        .1
        .parse::<u32>()
        .ok()?;
    (page > 1).then(|| format!("?page={page}"))
}

/// The site's default locale, given the raw `site_config` value and the installed
/// roster.
///
/// Falls back to [`Locale::default_locale`] when the value is absent, unparseable,
/// or names a locale that is **not installed**. That last case is the one worth
/// being deliberate about: honouring it would serve a language with no catalog, so
/// every string on the page would render as a raw key. `startup.rs` logs a WARN for
/// it, since a silently ignored setting is what made this key inert to begin with.
///
/// Pure, and separate from [`negotiate_locale`] for that reason — the decision is
/// what needs testing, and an `AppState` cannot be constructed in the test suite.
pub fn site_default_locale(raw: Option<&str>, installed: &[Locale]) -> Locale {
    raw.and_then(Locale::parse)
        .filter(|l| installed.contains(l))
        .unwrap_or_else(Locale::default_locale)
}

/// Resolves the request's locale and attaches it as an extension.
///
/// Precedence: path prefix > `?lang=` > cookie > `Accept-Language` > site default.
/// A signed-in user's stored preference is applied later by the auth layer and
/// overrides only the last three, so a `/vi/` link always wins.
///
/// Unrecognised values fall through rather than 400 — a stale cookie naming a
/// disabled locale must not lock the visitor out.
pub async fn negotiate_locale(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let enabled = state.translator.available_locales();
    // The in-memory config cache, not a query: this runs on every request, and
    // `update_config` writes the cache in the same request that writes the row, so
    // it cannot go stale.
    let site_default = site_default_locale(
        state
            .site_config_cache
            .read()
            .await
            .get(DEFAULT_LOCALE_KEY)
            .map(String::as_str),
        &enabled,
    );

    let from_path = locale_from_path(req.uri().path(), &enabled);
    let locale = from_path
        .clone()
        .or_else(|| locale_from_query(req.uri().query(), &enabled))
        .or_else(|| locale_from_cookie(&req, &enabled))
        .or_else(|| locale_from_accept_language(&req, &enabled))
        .unwrap_or_else(|| site_default.clone());

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
    let canonical_path = match canonical_query(req.uri().query()) {
        Some(q) => format!("{}{q}", req.uri().path()),
        None => req.uri().path().to_string(),
    };

    // Both are inserted: `translate_errors` only needs the locale, while page
    // handlers need the path and the unprefixed locale too. Keeping the bare
    // `Locale` avoids making the error path depend on a web-layer struct.
    req.extensions_mut().insert(locale.clone());
    req.extensions_mut().insert(RequestLocale {
        locale,
        canonical_path,
        site_default,
    });
    next.run(req).await
}

/// Rebuilds a URI with the leading `/{locale}` segment removed.
///
/// `/vi/forum/t/x?a=1` → `/forum/t/x?a=1`, and a bare `/vi` → `/`.
pub fn strip_locale_prefix(uri: &axum::http::Uri, tag: &str) -> Option<axum::http::Uri> {
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
pub fn locale_from_path(path: &str, enabled: &[Locale]) -> Option<Locale> {
    let first = path.strip_prefix('/')?.split('/').next()?;
    let candidate = Locale::parse(first)?;
    enabled.contains(&candidate).then_some(candidate)
}

pub fn locale_from_query(query: Option<&str>, enabled: &[Locale]) -> Option<Locale> {
    let query = query?;
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "lang")
        .and_then(|(_, v)| Locale::parse(v))
        .filter(|l| enabled.contains(l))
}

pub fn locale_from_cookie(req: &Request, enabled: &[Locale]) -> Option<Locale> {
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
pub fn locale_from_accept_language(req: &Request, enabled: &[Locale]) -> Option<Locale> {
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
