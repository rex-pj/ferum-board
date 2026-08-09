use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use axum::extract::Multipart;
use axum::http::HeaderMap;
use ferum_application::constants::MAX_PAGE;
use ferum_application::shared::AppError;

use crate::app_state::AppState;

/// Pull a single named part out of a multipart body, returning its bytes and
/// declared content type. Size, magic-byte and type validation belong to the
/// use case, which owns the per-feature limits.
///
/// This is the one place the `next_field` loop is written. Every handler that
/// wants exactly one uploaded part goes through here; handlers that read a
/// *multi-field* form (a compose form with title + body + thumbnail, or an
/// install form with a package + its capability grants) still drive the loop
/// themselves, because there is no single field for this to return and forcing
/// them through it would mean walking the body once per field.
///
/// Scanning stops at the first matching part. A body carrying two parts of the
/// same name is malformed for these endpoints either way; taking the first is
/// both the cheaper read and the harder one to abuse.
pub async fn read_file_field(
    multipart: &mut Multipart,
    field_name: &str,
) -> Result<(bytes::Bytes, String), AppError> {
    read_part(multipart, field_name, "file_field_missing").await
}

/// `read_file_field` for endpoints that accept an image, differing only in which
/// catalog sentence the caller gets when the part is absent
/// (`error-image-field-missing` rather than `error-file-field-missing`).
pub async fn read_image_field(
    multipart: &mut Multipart,
    field_name: &str,
) -> Result<(bytes::Bytes, String), AppError> {
    read_part(multipart, field_name, "image_field_missing").await
}

async fn read_part(
    multipart: &mut Multipart,
    field_name: &str,
    missing_code: &str,
) -> Result<(bytes::Bytes, String), AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some(field_name) {
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
            return Ok((data, content_type));
        }
    }
    Err(AppError::invalid(missing_code))
}

/// The three checks every image upload owes, in the order that makes them
/// meaningful: the declared type (which decides the stored file extension), the
/// size cap (per-feature, hence a parameter), and finally the magic bytes —
/// because a declared `image/png` proves nothing about the payload, and the CAS
/// key is derived from that payload.
///
/// The predicates themselves live in `ferum-application`; this is the ordering
/// and the error vocabulary, which is a web-layer concern.
///
/// `kind` selects the catalog sentence. Those per-feature keys were already
/// written in every locale but no upload handler used them: each had inlined its
/// own English literal (`"File exceeds 5 MB limit"`), which is both the
/// duplication and an i18n violation, since a Vietnamese visitor got the English
/// string. Passing the kind keeps the specific wording while there is still only
/// one copy of the logic.
pub fn validate_upload_image(
    content_type: &str,
    data: &[u8],
    max_bytes: usize,
    kind: ImageKind,
) -> Result<(), AppError> {
    if !ferum_application::storage_utils::validate_image_content_type(content_type) {
        return Err(AppError::invalid(kind.invalid_type));
    }
    if data.len() > max_bytes {
        return Err(AppError::invalid_with(
            kind.too_large,
            [("limit_mb", (max_bytes / (1024 * 1024)).into())],
        ));
    }
    // Checked last and deliberately: it is the only one that looks at the bytes
    // rather than what the client claimed about them.
    if !ferum_application::validators::validate_image_magic(data) {
        return Err(AppError::invalid(kind.invalid_type));
    }
    Ok(())
}

/// The pair of catalog codes an image upload can fail with.
///
/// Spelled out as `&'static str` literals rather than assembled from a `&str`
/// kind with `format!`. That is not stylistic: `tests/web/src/error_catalog.rs`
/// guarantees every emitted code has a catalog entry by *scanning the source for
/// single-line string literals*. A code built at runtime is invisible to that
/// scan, so a `format!` here would quietly exempt every upload error from the
/// one test that stops a raw key like `banner_too_large` reaching a user.
/// Written this way, adding a new kind without adding its two `.ftl` entries
/// fails the suite, in every installed locale.
#[derive(Clone, Copy)]
pub struct ImageKind {
    pub invalid_type: &'static str,
    pub too_large: &'static str,
}

impl ImageKind {
    pub const AVATAR: Self = Self {
        invalid_type: "avatar_invalid_type",
        too_large: "avatar_too_large",
    };
    pub const COVER: Self = Self {
        invalid_type: "cover_invalid_type",
        too_large: "cover_too_large",
    };
    pub const LOGO: Self = Self {
        invalid_type: "logo_invalid_type",
        too_large: "logo_too_large",
    };
    pub const THUMBNAIL: Self = Self {
        invalid_type: "thumbnail_invalid_type",
        too_large: "thumbnail_too_large",
    };
}

// ─── Pagination ───────────────────────────────────────────────────────────────

/// Normalises the `?page=` / `?per_page=` pair from an untrusted query string.
///
/// `default_per_page` and `max_per_page` stay arguments rather than constants
/// because they are genuinely per-endpoint policy (a user lookup caps lower
/// than a thread feed). What is NOT per-endpoint is the pair of invariants this
/// exists to hold: `page` is 1-based and must never reach the repository as 0
/// (offset is computed as `(page - 1) * per_page`, so a 0 underflows on
/// unsigned arithmetic), and `per_page` must never be 0 (a LIMIT of nothing).
/// Both were previously re-derived by hand at 26 call sites.
///
/// This is an input bound, not the final say: the use cases additionally clamp
/// to the admin's `max_threads_per_page` / `max_posts_per_page` site config,
/// which is the layer allowed to make that decision.
///
/// `page` is rejected rather than clamped once it passes [`MAX_PAGE`]. Clamping
/// would be a one-line change here and no change at all in the callers, but it
/// answers a request for page 999999 with the contents of page 500 and calls it
/// success — the reader is told nothing, and a crawler keeps walking. Returning
/// an error costs the `?` at each call site and makes the ceiling visible in
/// both the JSON envelope and the HTML error page, in the reader's language.
pub fn paginate(
    page: Option<u64>,
    per_page: Option<u64>,
    default_per_page: u64,
    max_per_page: u64,
) -> Result<(u64, u64), AppError> {
    Ok((
        page_number(page)?,
        per_page.unwrap_or(default_per_page).clamp(1, max_per_page),
    ))
}

/// The page half of [`paginate`], for the handlers that have no `per_page` to
/// negotiate — a page whose page size is fixed in code.
///
/// Those handlers wrote `q.page.unwrap_or(1).max(1)` inline, which is this
/// function's first line and none of its second: they accepted `?page=999999`
/// and paid for the `OFFSET` that implies. Splitting the guard out is what lets
/// them adopt it without inventing a `per_page` they do not have.
///
/// HTML page handlers map the error to `PageError::NotFound` rather than
/// letting `From<AppError>` collapse it into `Internal`. A page past the
/// ceiling does not exist, so 404 is both the honest answer and the one that
/// stops a crawler; a 500 would say the server broke.
pub fn page_number(page: Option<u64>) -> Result<u64, AppError> {
    let page = page.unwrap_or(1).max(1);
    if page > MAX_PAGE {
        return Err(AppError::invalid_with(
            "page_out_of_range",
            [("max_page", MAX_PAGE.into())],
        ));
    }
    Ok(page)
}

// ─── Content-addressed caching ────────────────────────────────────────────────
//
// A CAS key is a digest of the bytes it names, so it doubles as a strong ETag:
// the content behind a key can never change. That is what lets a handler answer
// `If-None-Match` from the key alone, without reading the row it refers to.

/// The `ETag` a CAS key implies, quoted per RFC 9110.
pub fn etag_for(key: &str) -> String {
    format!("\"{key}\"")
}

/// The single-header array to attach to a `304`, so callers do not re-spell it.
pub fn etag_headers(key: &str) -> [(axum::http::HeaderName, String); 1] {
    [(axum::http::header::ETAG, etag_for(key))]
}

/// Whether the client already holds the content named by `key`.
///
/// `If-None-Match` is a comma-separated list and may be `*`. Weak validators
/// (`W/"…"`) compare equal for the purposes of a 304 and browsers do echo them
/// back, so the `W/` prefix is stripped rather than treated as a miss.
pub fn if_none_match_hits(headers: &HeaderMap, key: &str) -> bool {
    let Some(raw) = headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let want = etag_for(key);
    raw.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*" || candidate.trim_start_matches("W/") == want
    })
}

// ─── Auth cookies ─────────────────────────────────────────────────────────────
// Single source for the auth Set-Cookie strings. Max-Age comes from the
// TokenService TTLs (JWT_EXPIRY_SECONDS / REFRESH_TOKEN_EXPIRY_DAYS) so the
// cookie lifetime always matches the token's `exp` claim.

/// Builds one auth `Set-Cookie` value.
///
/// Split out from the `AppState`-taking wrappers below so the attribute string
/// can be asserted in a unit test without standing up an `AppState`. That
/// matters more than it looks: `SameSite=Lax` is load-bearing CSRF protection
/// here — there is no CSRF token, so weakening it to `None` would leave the
/// `Origin` check as the only defence (see the CSRF section in CLAUDE.md).
pub fn build_auth_cookie(
    name: &str,
    value: &str,
    path: &str,
    max_age_secs: u64,
    secure: bool,
) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{name}={value}; HttpOnly; SameSite=Lax; Path={path}; Max-Age={max_age_secs}{secure}")
}

/// `token=…` access-token cookie (Path=/).
pub fn access_token_cookie(state: &AppState, token: &str) -> String {
    build_auth_cookie(
        "token",
        token,
        "/",
        state.token_service.access_token_ttl_secs(),
        state.cookies_secure,
    )
}

/// `refresh_token=…` cookie, scoped to the auth endpoints only.
pub fn refresh_token_cookie(state: &AppState, token: &str) -> String {
    build_auth_cookie(
        "refresh_token",
        token,
        "/api/auth",
        state.token_service.refresh_token_ttl_secs(),
        state.cookies_secure,
    )
}

/// Expired `token=` cookie (logout).
pub fn clear_access_token_cookie(state: &AppState) -> String {
    build_auth_cookie("token", "", "/", 0, state.cookies_secure)
}

/// Expired `refresh_token=` cookie (logout).
pub fn clear_refresh_token_cookie(state: &AppState) -> String {
    build_auth_cookie("refresh_token", "", "/api/auth", 0, state.cookies_secure)
}

/// Builds a 16-hex-char fingerprint for a guest viewer from IP + User-Agent.
///
/// - IP: read from `X-Forwarded-For` (Nginx) or `X-Real-IP`.
/// - User-Agent: tells apart different browsers on the same IP (e.g. an office behind NAT).
/// - Uses std DefaultHasher — no crypto needed, just enough dispersion.
/// - Returns `None` when both IP and UA are empty (not enough signal to dedup).
pub fn guest_fingerprint(headers: &HeaderMap) -> Option<String> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
        })
        .unwrap_or("");

    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if ip.is_empty() && ua.is_empty() {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    ip.hash(&mut hasher);
    ua.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// `ferum_locale=…` cookie carrying the visitor's chosen language.
///
/// Deliberately **not** `HttpOnly`: unlike the auth token this is not a secret,
/// and the client-side switcher needs to read it to show which language is
/// active before any JS state exists.
///
/// For a signed-in user this cookie is a *cache* of `user_preferences.locale`,
/// refreshed at login and whenever the preference changes. Keeping the durable
/// copy in the database is what makes the choice follow the account to a new
/// device; keeping the request-time copy in a cookie is what avoids a database
/// lookup on every single page render just to know which language to draw.
pub fn locale_cookie(state: &AppState, locale: &str) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    // One year: a language choice is not session state, and re-picking it on
    // every visit would be worse than the marginal privacy cost.
    format!("{}={locale}; SameSite=Lax; Path=/; Max-Age=31536000{secure}",
        crate::middleware::locale::LOCALE_COOKIE)
}

/// Expired `ferum_locale=` cookie — returns the visitor to negotiated default.
pub fn clear_locale_cookie(state: &AppState) -> String {
    let secure = if state.cookies_secure { "; Secure" } else { "" };
    format!("{}=; SameSite=Lax; Path=/; Max-Age=0{secure}",
        crate::middleware::locale::LOCALE_COOKIE)
}

/// Resolve a `?category_id=` query value into a product listing filter.
///
/// The ids here are **catalogue** categories (Sofa, Ghế, Bàn …), not the
/// forum's discussion tree. The two are separate taxonomies on purpose: no
/// forum category is a sensible home for a sofa, which is why the column that
/// once pointed at them could never be populated.
///
/// Three shapes, because a plain `Option<Uuid>` cannot express all of them:
/// absent means no restriction, the literal `none` means *products with no
/// category* (the curator's queue — see [`CategoryFilter::Unassigned`]), and a
/// UUID means that category **and its children**.
///
/// The subtree expansion is the point of doing this here rather than in the
/// query struct: the tree is two levels, a reader filtering by a parent expects
/// the children's products, and resolving it per-surface is how the surfaces
/// drift apart.
///
/// An unknown or unparseable id yields `Any` rather than an error — a stale
/// link should widen, not 400.
pub async fn resolve_product_category_filter(
    state: &AppState,
    raw: Option<&str>,
) -> ferum_domain::repositories::product_repository::CategoryFilter {
    use ferum_domain::repositories::product_repository::CategoryFilter;

    let raw = raw.map(str::trim).filter(|s| !s.is_empty());
    let Some(raw) = raw else {
        return CategoryFilter::Any;
    };
    if raw.eq_ignore_ascii_case("none") {
        return CategoryFilter::Unassigned;
    }
    let Ok(id) = raw.parse::<uuid::Uuid>() else {
        return CategoryFilter::Any;
    };

    let all = state.product.list_categories().await.unwrap_or_default();
    let subtree: Vec<uuid::Uuid> = all
        .iter()
        .filter(|c| c.id == id || c.parent_id == Some(id))
        .map(|c| c.id)
        .collect();

    if subtree.is_empty() {
        CategoryFilter::Any
    } else {
        CategoryFilter::In(subtree)
    }
}

/// The same resolution, as a plain id list for the search port's
/// `in_category_ids` (empty = no restriction).
pub async fn resolve_product_category_ids(
    state: &AppState,
    raw: Option<&str>,
) -> Vec<uuid::Uuid> {
    use ferum_domain::repositories::product_repository::CategoryFilter;

    match resolve_product_category_filter(state, raw).await {
        CategoryFilter::In(ids) => ids,
        // "Unassigned" has no representation in the search index — a product
        // with no category simply has no category id to match — and search is
        // a reader surface, where that filter has no meaning anyway.
        CategoryFilter::Any | CategoryFilter::Unassigned => Vec::new(),
    }
}
