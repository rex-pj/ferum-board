pub mod account;
pub mod auth;
pub mod catalog;
pub mod compose;
pub mod forum;
pub mod profile;
pub mod search;
pub mod setup;
pub mod sitemap;
pub mod thread_permissions;

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use tera::Context;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use ferum_application::constants::DEFAULT_THEME_SLUG;
use ferum_application::shared::AppError;
use ferum_domain::models::{PostPolicy, ViewPolicy};
use crate::view_models::page_context::{
    CurrentUserCtx, NavCategoryCtx, PluginSlotCtx, TagCtx, ThreadCtx,
};

// ─── Shared query types ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PageQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

pub fn post_policy_str(p: &PostPolicy) -> String {
    match p {
        PostPolicy::Members   => "members",
        PostPolicy::Trusted   => "trusted",
        PostPolicy::StaffOnly => "staff_only",
        PostPolicy::Closed    => "closed",
        PostPolicy::Moderated => "moderated",
    }
    .to_string()
}

pub fn view_policy_str(p: &ViewPolicy) -> String {
    match p {
        ViewPolicy::Public      => "public",
        ViewPolicy::MembersOnly => "members_only",
        ViewPolicy::StaffOnly   => "staff_only",
    }
    .to_string()
}

/// Unwraps `Option<AuthUser>` for page handlers — redirects to `/login` on failure.
pub fn require_page_auth(auth_user: Option<AuthUser>) -> Result<AuthUser, PageError> {
    auth_user.ok_or(PageError::Unauthorized)
}

/// The `/login` redirect for a member-only page, carrying a `next` pointer so a
/// signed-out visitor lands on the page they asked for instead of the homepage.
///
/// `require_page_auth` above is the admin/mod counterpart and deliberately does
/// NOT carry `next` — those pages send the visitor to a bare `/login`. The two
/// are separate on purpose; this one exists so the `?next=` shape has a single
/// definition rather than being spelled out at each member page that needs it.
///
/// `next` is expected to be an in-site path — a literal (`/account`) or one
/// built from a slug (`/edit-thread/{slug}`). It is not percent-encoded,
/// because slugs are already URL-safe by construction; feeding this a
/// user-supplied string would want encoding first.
pub(crate) fn login_redirect(next: &str) -> Response {
    axum::response::Redirect::to(&format!("/login?next={next}")).into_response()
}

pub(super) async fn active_theme(state: &AppState) -> String {
    let _lat = crate::telemetry::Latency::start("active_theme");
    let slug = state.active_theme_cache.read().await.clone();
    crate::telemetry::record_cache_result("active_theme", "rwlock", true);
    slug
}

pub(super) async fn user_ctx(
    state: &AppState,
    auth_user: Option<&AuthUser>,
) -> Option<CurrentUserCtx> {
    let u = auth_user?;
    let (unread, prefs) = tokio::join!(
        state.notification.unread_count(u),
        state.user.get_preferences_by_id(u.id),
    );
    let mut ctx = CurrentUserCtx::from_auth(u, unread.unwrap_or(0));
    if let Ok(p) = prefs {
        ctx.theme = p.theme;
        ctx.font_size = p.font_size;
        ctx.layout = p.layout;
        ctx.timezone = p.timezone;
    }
    Some(ctx)
}

/// Fills in `timezone` on a context built by `CurrentUserCtx::from(&AuthUser)`.
///
/// The admin and moderator panels build their user context from the JWT, which
/// carries no preferences — so `timezone` was `None` on all ~22 of those pages
/// and `<meta name="ferum-tz">` never rendered there. An operator who set a zone
/// on their account got it on the forum and not in the panels, which is where
/// exact timestamps matter most: the audit log and the plugin log.
///
/// One indexed lookup on `user_preferences`, and only on HTML page renders for
/// an authenticated staff user — not on the API paths. It deliberately does not
/// fetch the unread count the public `user_ctx` does, because no panel template
/// shows it.
pub async fn with_viewer_timezone(
    state: &AppState,
    auth_user: &AuthUser,
    mut ctx: CurrentUserCtx,
) -> CurrentUserCtx {
    if let Ok(p) = state.user.get_preferences_by_id(auth_user.id).await {
        ctx.timezone = p.timezone;
    }
    ctx
}

#[tracing::instrument(skip_all)]
pub async fn nav_categories_ctx(
    state: &AppState,
    auth_user: Option<&AuthUser>,
) -> Vec<NavCategoryCtx> {
    let cats = state
        .category
        .list_visible(auth_user)
        .await
        .unwrap_or_default();

    let mut children_map: std::collections::HashMap<String, Vec<NavCategoryCtx>> =
        std::collections::HashMap::new();
    let mut parents: Vec<(String, NavCategoryCtx)> = Vec::new();

    for c in &cats {
        let entry = NavCategoryCtx {
            slug: c.slug.clone(),
            name: c.name.clone(),
            color: c.color.clone(),
            children: Vec::new(),
        };
        if let Some(pid) = c.parent_id {
            children_map.entry(pid.to_string()).or_default().push(entry);
        } else {
            parents.push((c.id.to_string(), entry));
        }
    }

    parents
        .into_iter()
        .map(|(id, mut p)| {
            p.children = children_map.remove(&id).unwrap_or_default();
            p
        })
        .collect()
}

pub(super) async fn plugin_ctx_data(
    state: &AppState,
) -> (
    std::collections::HashMap<String, Vec<PluginSlotCtx>>,
    Vec<String>,
) {
    let slots = state.plugin_ui.active_ui_slots().await;
    let mut map: std::collections::HashMap<String, Vec<PluginSlotCtx>> =
        std::collections::HashMap::new();
    let mut assets: Vec<String> = Vec::new();
    let mut seen_assets: std::collections::HashSet<String> = std::collections::HashSet::new();
    for slot in slots {
        let attrs = slot.props.join(" ");
        let html = if attrs.is_empty() {
            format!("<{0}></{0}>", slot.custom_element_tag)
        } else {
            format!("<{0} {1}></{0}>", slot.custom_element_tag, attrs)
        };
        if seen_assets.insert(slot.asset_url.clone()) {
            assets.push(slot.asset_url);
        }
        map.entry(slot.slot_name)
            .or_default()
            .push(PluginSlotCtx { html });
    }
    (map, assets)
}

/// Strings the browser renders itself, as `key → text` in the given locale.
///
/// Scoped to the `js-` namespace deliberately. Shipping the whole catalog would
/// put every string — including admin copy a visitor can never see — into every
/// page's HTML, and it would grow without bound as the site is translated.
///
/// This used to *build* the dictionary here, on every render: clone the entire
/// default key set (`ui-`, `js-`, `adm-` and `error-` alike), filter it, then
/// Fluent-format each survivor. It is now resolved once per catalog load inside
/// the translator and handed back as an `Arc` — see [`Translator::js_strings`].
pub fn js_strings_for(
    state: &AppState,
    locale: &ferum_domain::Locale,
) -> std::sync::Arc<std::collections::BTreeMap<String, String>> {
    state.translator.js_strings(locale)
}

/// Renders a themed page in the default locale.
///
/// Retained for callers that have no request locale to hand; prefer
/// [`render_with_theme_in`] on any path that serves a visitor.
pub async fn render_with_theme(
    state: &AppState,
    active_theme: &str,
    page_key: &str,
    ctx: Context,
) -> Result<Html<String>, PageError> {
    render_with_theme_in(
        state,
        &crate::middleware::locale::RequestLocale::default(),
        active_theme,
        page_key,
        ctx,
    )
    .await
}

/// Renders a themed page in an explicit locale.
///
/// The locale does two things: it picks the compiled template set whose `t()`
/// resolves against that language's catalog, and it is exposed to templates as
/// `locale` so `base.html` can emit `<html lang="…">` and build `hreflang`
/// alternates.
#[tracing::instrument(skip(state, ctx, req_locale), fields(theme = active_theme, page_key, locale = %req_locale.locale))]
pub async fn render_with_theme_in(
    state: &AppState,
    req_locale: &crate::middleware::locale::RequestLocale,
    active_theme: &str,
    page_key: &str,
    mut ctx: Context,
) -> Result<Html<String>, PageError> {
    let locale = &req_locale.locale;
    let (plugin_slots, plugin_assets) = plugin_ctx_data(state).await;
    let theme_bs_theme = state.active_theme_color_scheme_cache.read().await.clone();
    ctx.insert("plugin_slots", &plugin_slots);
    ctx.insert("plugin_assets", &plugin_assets);
    ctx.insert("theme_bs_theme", &theme_bs_theme);
    ctx.insert("locale", locale.as_str());
    ctx.insert("current_path", &req_locale.canonical_path);
    ctx.insert("js_strings", &js_strings_for(state, locale));
    // Field limits the server enforces, so a `maxlength` attribute cannot drift
    // away from the validator behind it. Injected here rather than per-page
    // because a limit is the same on every request and any theme may need it.
    ctx.insert("limits", &crate::view_models::page_context::FieldLimitsCtx::new());
    ctx.insert(
        "available_locales",
        &state
            .translator
            .available_locales()
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>(),
    );

    let chain = state.active_theme_chain_cache.read().await.clone();
    let candidates: Vec<String> = chain
        .iter()
        .map(|slug| format!("{}/templates/{}", slug, page_key))
        .collect();
    let template_name = state
        .tera
        .first_existing_template(&candidates)
        .await
        .unwrap_or_else(|| format!("{}/templates/{}", DEFAULT_THEME_SLUG, page_key));

    let html = state.tera.render(locale, &template_name, ctx).await?;
    Ok(Html(html))
}

pub(super) fn map_threads(
    threads: &[ferum_domain::models::Thread],
) -> Vec<ThreadCtx> {
    let empty_r = std::collections::HashMap::new();
    let empty_i = std::collections::HashMap::new();
    map_threads_with_ratings(threads, &empty_r, &empty_i)
}

/// Like `map_threads`, but attaches each review's overall score and reviewed
/// product from pre-loaded `thread_id → …` maps (batch-fetched by the caller —
/// no N+1).
pub(super) fn map_threads_with_ratings(
    threads: &[ferum_domain::models::Thread],
    ratings: &std::collections::HashMap<uuid::Uuid, i16>,
    products: &std::collections::HashMap<
        uuid::Uuid,
        ferum_domain::repositories::product_repository::ReviewedProduct,
    >,
) -> Vec<ThreadCtx> {
    threads
        .iter()
        .map(|t| {
            let author = t.author_username.clone().unwrap_or_default();
            ThreadCtx {
                id: t.id.to_string(),
                slug: t.slug.clone(),
                title: t.title.clone(),
                author_username: author.clone(),
                author_display_name: t
                    .author_display_name
                    .clone()
                    .unwrap_or_else(|| author.clone()),
                author_avatar_url: t.author_avatar_url.clone(),
                category_slug: t.category_slug.clone(),
                category_name: t.category_name.clone().unwrap_or_default(),
                is_review: t.category_slug
                    == ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG,
                review_overall: ratings.get(&t.id).copied(),
                review_product_image: products
                    .get(&t.id)
                    .and_then(|p| p.primary_image_key.clone()),
                review_product_name: products.get(&t.id).map(|p| p.name.clone()),
                review_product_slug: products.get(&t.id).map(|p| p.slug.clone()),
                reply_count: t.reply_count,
                view_count: t.view_count,
                is_pinned: t.is_pinned,
                is_solved: t.is_solved,
                is_locked: matches!(t.status, ferum_domain::models::ThreadStatus::Locked),
                status: format!("{:?}", t.status).to_lowercase(),
                thumbnail_url: t.thumbnail_url.clone(),
                excerpt: t.excerpt.clone(),
                last_post_at: t.last_post_at.map(|d| d.to_rfc3339()),
                created_at: t.created_at.to_rfc3339(),
                tags: t
                    .tags
                    .iter()
                    .map(|tag| TagCtx {
                        name: tag.name.clone(),
                        slug: tag.slug.clone(),
                        color: tag.color.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Batch-load the overall score for every review thread in `threads`, keyed by
/// thread id. One query for the whole page (no N+1); empty when none are reviews.
pub(super) async fn review_overall_map(
    state: &crate::app_state::AppState,
    threads: &[ferum_domain::models::Thread],
) -> std::collections::HashMap<uuid::Uuid, i16> {
    use ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG;
    let ids: Vec<uuid::Uuid> = threads
        .iter()
        .filter(|t| t.category_slug == REVIEWS_CATEGORY_SLUG)
        .map(|t| t.id)
        .collect();
    if ids.is_empty() {
        return std::collections::HashMap::new();
    }
    state
        .review
        .ratings_for_threads(&ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(id, r)| (id, r.overall))
        .collect()
}

/// Batch-load `review_thread_id → reviewed product` for the review threads in
/// `threads`, so review cards can name and link their subject and fall back to
/// its cover image. One query; empty when none are reviews.
pub(super) async fn review_product_map(
    state: &crate::app_state::AppState,
    threads: &[ferum_domain::models::Thread],
) -> std::collections::HashMap<
    uuid::Uuid,
    ferum_domain::repositories::product_repository::ReviewedProduct,
> {
    use ferum_application::usecases::category_usecase::REVIEWS_CATEGORY_SLUG;
    let ids: Vec<uuid::Uuid> = threads
        .iter()
        .filter(|t| t.category_slug == REVIEWS_CATEGORY_SLUG)
        .map(|t| t.id)
        .collect();
    if ids.is_empty() {
        return std::collections::HashMap::new();
    }
    state.product.reviewed_products(&ids).await.unwrap_or_default()
}

// ─── Static fallback HTML ─────────────────────────────────────────────────────
// Used when Tera is unavailable (e.g. during a panic or DB-down scenario).
// Never contains user input — safe to serve unconditionally.

static STATIC_404_HTML: &str = include_str!("../../error_pages/404_static.html");
static STATIC_ERROR_HTML: &str = include_str!("../../error_pages/error_static.html");

// ─── Error page render helpers ────────────────────────────────────────────────

pub async fn render_404_page(
    state: &AppState,
    req_locale: &crate::middleware::locale::RequestLocale,
    auth_user: Option<&AuthUser>,
) -> Response {
    let active = active_theme(state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &crate::handlers::admin::site_ctx(state).await);
    ctx.insert("active_theme", &active);
    ctx.insert("current_user", &user_ctx(state, auth_user).await);
    match render_with_theme_in(state, req_locale, &active, "errors/404.html", ctx).await {
        Ok(html) => (StatusCode::NOT_FOUND, html).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Html(STATIC_404_HTML)).into_response(),
    }
}

pub async fn render_error_page(
    state: &AppState,
    req_locale: &crate::middleware::locale::RequestLocale,
    auth_user: Option<&AuthUser>,
) -> Response {
    let active = active_theme(state).await;
    let mut ctx = Context::new();
    ctx.insert("site", &crate::handlers::admin::site_ctx(state).await);
    ctx.insert("active_theme", &active);
    ctx.insert("current_user", &user_ctx(state, auth_user).await);
    match render_with_theme_in(state, req_locale, &active, "errors/error.html", ctx).await {
        Ok(html) => (StatusCode::INTERNAL_SERVER_ERROR, html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Html(STATIC_ERROR_HTML)).into_response(),
    }
}

// ─── Error type ───────────────────────────────────────────────────────────────

pub enum PageError {
    Internal(anyhow::Error),
    /// The *caller* got it wrong — a corrupt archive, a manifest that will not
    /// parse, a file over the size cap. Distinct from `Internal` because the
    /// admin can act on it, so the reason has to reach them: these endpoints are
    /// fetched by JS that renders the response into a modal, and a 500 with the
    /// generic error page tells the person holding the bad file nothing at all.
    BadRequest(String),
    Unauthorized,
    NotFound,
}

impl From<anyhow::Error> for PageError {
    fn from(e: anyhow::Error) -> Self {
        PageError::Internal(e)
    }
}

impl From<AppError> for PageError {
    fn from(e: AppError) -> Self {
        PageError::Internal(anyhow::anyhow!("{:?}", e))
    }
}

/// Marks a response whose body is already a message meant for the caller, so
/// `error_page_layer` leaves it alone instead of substituting the themed error
/// page. Without it the reason an upload was rejected is replaced by a generic
/// "Something went wrong" before it ever reaches the modal that asked for it.
#[derive(Clone, Copy)]
pub struct ClientErrorPassthrough;

/// Converts a use-case error into a `PageError` that preserves its message.
///
/// The `From<AppError>` impl above collapses everything into `Internal`, which
/// is right for a page navigation — a visitor cannot act on "conflict:
/// slug_taken" — and wrong for the admin upload endpoints, which are fetched by
/// JS and render the response into a dialog. There the catalog already holds a
/// usable sentence for every coded error; this resolves it in the request's
/// locale, exactly as `translate_errors` does for the JSON API.
///
/// Server errors stay `Internal`: their detail is for the log, not the browser.
pub fn client_facing_error(
    state: &AppState,
    locale: &ferum_domain::Locale,
    e: AppError,
) -> PageError {
    if e.status_and_code().0.is_server_error() {
        return PageError::Internal(anyhow::anyhow!("{:?}", e));
    }
    let message = match e.error_payload() {
        Some(payload) => state.translator.translate(
            locale,
            &payload.key(),
            payload.translator_args().as_slice(),
        ),
        None => e.fallback_message(),
    };
    PageError::BadRequest(message)
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        match self {
            PageError::Internal(e) => {
                tracing::error!("Page render error: {:?}", e);
                // Never render via Tera here — Tera itself may have caused this error.
                (StatusCode::INTERNAL_SERVER_ERROR, Html(STATIC_ERROR_HTML)).into_response()
            }
            PageError::BadRequest(msg) => {
                // Plain text, not the HTML error page: the callers of these
                // endpoints put the body straight into an alert box.
                let mut res = (StatusCode::BAD_REQUEST, msg).into_response();
                res.extensions_mut().insert(ClientErrorPassthrough);
                res
            }
            PageError::Unauthorized => {
                axum::response::Redirect::to("/login").into_response()
            }
            PageError::NotFound => {
                // No AppState here; static fallback is acceptable for in-handler NotFound.
                (StatusCode::NOT_FOUND, Html(STATIC_404_HTML)).into_response()
            }
        }
    }
}
