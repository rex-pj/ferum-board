# Internationalization (i18n) Plan — Ferum Board

## Status: Implemented and running

**Landed (2026-07-21):** Steps 1–4 backend. `Locale` value type, `Translator` port,
`FluentTranslator` with Arc-swap reload, the full English error catalog, ~105 error
call sites converted from prose to codes, locale negotiation + subpath stripping,
per-locale Tera instances with `t()`, and the error-translation response middleware.
59 new tests; whole suite green.

**Landed (part 2):** `user_preferences.locale` + migration, `PUT /api/locale` for
guests, the nav and account switchers, `<html lang>` + `hreflang`, localized
transactional emails, `/admin/languages` (Step 7 Tier 1) behind a new
`admin.languages` permission, and a real partial Vietnamese catalog. 580 tests green.

**Landed (part 3):** bulk extraction — 476 public + 906 admin/mod + 69 theme strings;
client-side `Ferum.t()`; catalog-driven date/number formatting; localized HTML error
pages; theme-owned catalogs discovered at startup; five template-integrity tests.
**592 tests green.**

**Landed (part 4):** fixed two runtime defects no test caught — LOCALES_DIR resolving
to a non-existent path (every page rendered raw keys) and `/vi/…` 404ing because
`Router::layer` runs after routing. Startup now fails closed on an empty catalog, and
`template_keys.rs` asserts every `t()` key in every template resolves. Plus four UI
fixes. **594 tests green.**

**Outstanding:** ~74 mixed text+interpolation nodes need a Fluent placeable each,
page `{% block title %}` copy, admin translations (extracted but English-only by
decision), and Step 7 Tiers 2–3 (language-pack upload, string override editor).
Deviations from the plan as written are recorded in "Implementation Notes" at the end.

This document captures the multi-language audit findings and the concrete steps to add locale support. The architecture is unusually well-positioned: error codes, notification payloads, and the theme-chain resolver already provide the seams i18n needs. The work concentrates in two places — a 60-site error refactor that is worth doing on its own merits, and mechanical string extraction from templates.

---

## Locked Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| URL strategy | **Subpath prefix** (`/vi/forum/t/slug`) | Lurkers are 70% of the audience and NF-SE-01..04 make SEO explicit. Distinct crawlable URLs per locale; cache key falls out of the URL with no `Vary` gymnastics. |
| Text direction | **LTR only**, 2–4 locales | No Bootstrap RTL bundle, no logical-property migration of `fr-*` theme CSS. |
| Admin / mod panels | **Structured now, translated later** | The 567 admin strings get extracted into catalogs but ship English-only. Adding a locale later becomes data-only, no code change. |
| Library | **Fluent** (`fluent-bundle`) | See Step 2. |
| User-generated content | **Out of scope** | Threads and posts stay in their authored language. See "Explicitly Out of Scope". |

---

## Audit Findings Summary

| Area | Status | Detail |
|------|--------|--------|
| Error codes → message catalog | ✅ Ready | `human_message(code)` in `ferum-domain/src/error.rs` — 87 coded sites translate via one edit |
| Notification text | ✅ Ready | `kind` + JSONB payload composed in Tera; **no English persisted in DB** — existing rows translate retroactively |
| Single render funnel | ✅ Ready | `render_with_theme()` is the only path for public pages — one locale injection point |
| Theme chain resolver | ✅ Reusable | Child→parent template fallback is structurally identical to `vi`→`en` string fallback |
| Hot-reload + Arc-swap | ✅ Reusable | `TeraEngine::reload_themes()` pattern applies verbatim to catalog reload |
| Postgres FTS | ✅ Neutral | Trigger uses `to_tsvector('simple', …)` — no English stemming baked in |
| Free-text errors | ❌ Blocker | 60 `unprocessable("English prose")` sites bypass the code catalog |
| Template strings | ❌ Work | ~392 public + ~567 admin/mod hardcoded strings across 60 files / 10.3k lines |
| Client JS strings | ❌ Work | ~3.5k lines across `ferum-page-*.js`, `ferum-admin*.js`, and Svelte widgets |
| Date / number formatting | ❌ Work | `date(format="%b %d, %Y")` in ~10 templates; `group_thousands()` hardcodes separator |
| `<html lang>` | ❌ Work | Hardcoded `lang="en"` in every theme's `base.html` |
| Email locale | ❌ Blocker | `ForumJob::Send*Email` carries no locale; worker runs detached from request |
| Guest SSR cache | ⚠️ Risk | Nginx cache keyed on `Vary: Cookie` — will serve wrong-language pages once SSR is locale-dependent |
| Theme / plugin catalogs | ⚠️ Design | Third-party themes and plugins ship their own strings; must be designed in, not retrofitted |

---

## Step 1 — Convert free-text errors to codes

### Problem

Two error idioms coexist and only one is translatable:

```rust
AppError::forbidden("thread_locked")                      // 87 sites — code, translatable
AppError::unprocessable("Post content cannot be empty")   // 60 sites — prose, untranslatable
```

The prose variant emits raw English straight into the JSON API response body. This is also an API-contract defect independent of i18n: every one of these returns `code: "validation_error"`, so a client cannot branch on the actual failure — the meaning lives only in the sentence.

### Solution

Convert all 60 sites to codes and extend the catalog. Where a message carries a runtime value, the code becomes a key plus named arguments rather than a formatted string:

```rust
// Before
AppError::unprocessable("Post content exceeds 100 KB limit")

// After
AppError::unprocessable_with("post_too_long", &[("limit_kb", 100)])
```

`human_message` then becomes a thin delegate to the `Translator` port (Step 2), and the fallback for uncatalogued codes stays exactly as it is today — a space-separated rendering, so new codes never panic.

**Do this step first.** It is the highest-leverage change in the effort, it is independently justified as an API-contract fix, and every later step depends on error text having a stable key.

---

## Step 2 — `Translator` port + Fluent adapter

### Why Fluent, not `rust-i18n`

The decisive reason is correctness, not features. `rust-i18n`'s `t!` macro resolves the active locale from ambient thread-local state. Under Tokio work-stealing with a per-request locale, that is a data hazard — a future can migrate between worker threads mid-render and pick up another request's locale.

Fluent bundles are plain data keyed by an explicit locale, which composes cleanly with request scope. Fluent also implements CLDR plural categories, which a forum needs constantly (`1 reply` / `2 replies`, Russian's 3 forms, Arabic's 6).

### Layer placement

Respects the strict dependency rules in CLAUDE.md — dependencies flow inward only:

```
ferum-domain          Locale value type (validated BCP-47). Pure, no I/O.
ferum-application     Translator port trait in ports.rs. Use cases stay infra-free.
ferum-infrastructure  FluentTranslator — owns fluent-bundle, .ftl loading, Arc-swap reload.
ferum-web             Negotiation middleware, per-locale Tera instances, subpath routing.
```

**Domain** (`ferum-domain/src/locale.rs`):

```rust
/// A validated, supported locale. Construction is fallible so an
/// attacker-controlled path segment can never reach the bundle lookup.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Locale(&'static str);

impl Locale {
    pub const DEFAULT: Locale = Locale("en");

    pub fn parse(tag: &str) -> Option<Locale> { /* match against SUPPORTED */ }
    pub fn as_str(&self) -> &'static str { self.0 }
}
```

**Application** (`ferum-application/src/ports.rs`):

```rust
pub trait Translator: Send + Sync {
    /// Never fails: an unknown key falls back through the locale chain to
    /// the default locale, then to the key itself.
    fn translate(&self, locale: Locale, key: &str, args: &FluentArgs) -> String;
}
```

**Infrastructure** (`ferum-infrastructure/src/i18n/fluent_translator.rs`) holds `Arc<RwLock<HashMap<Locale, FluentBundle<FluentResource>>>>` and reloads via the same `spawn_blocking` + Arc-swap shape as `TeraEngine::reload_themes()`.

---

## Step 3 — Per-locale Tera instances

### Problem

Tera 1.x global functions receive their arguments but **cannot read the render context**. So a single `t()` function registered on a shared `Arc<Tera>` has no way to see a per-request locale. The naive workaround forces `{{ t(k="thread.reply", loc=locale) }}` on all ~960 call sites.

A `tokio::task_local!` would keep call sites clean and is safe today, but it breaks silently the moment `render()` moves to `spawn_blocking`. Rejected as a latent trap.

### Solution

Build **one Tera instance per locale** at startup, each with a `t()` closure bound to that locale:

```rust
pub struct TeraEngine {
    // was: Arc<RwLock<Arc<Tera>>>
    instances: Arc<RwLock<HashMap<Locale, Arc<Tera>>>>,
    translator: Arc<dyn Translator>,
}
```

`build_tera()` gains a `locale` parameter and registers:

```rust
tera.register_function("t", move |args: &HashMap<String, Value>| {
    let key = args.get("k").and_then(Value::as_str).ok_or(/* … */)?;
    Ok(Value::String(translator.translate(locale, key, &to_fluent_args(args))))
});
```

Trade-offs, stated plainly:

- **Call sites stay clean** — `{{ t(k="thread.reply") }}`, no locale noise on 960 strings.
- **Memory cost is negligible** — 10.3k lines of templates parsed N times is a few MB for N ≤ 4.
- **Rebuild cost is N×** — already on `spawn_blocking` with atomic swap, so no request-path impact.
- Reuses the exact hot-reload machinery already proven for themes.

`render_with_theme()` selects the instance by the request's `Locale` extension. The theme-chain candidate resolution is unchanged.

---

## Step 4 — Locale negotiation + subpath routing

### Negotiation order

1. Path prefix (`/vi/…`) — authoritative when present
2. User preference (`user_preferences.locale`)
3. `?lang=` query override — sets the cookie, then redirects to the canonical subpath
4. `ferum_locale` cookie
5. `Accept-Language` header (via `fluent-langneg`)
6. Site default

### Who can change the language

**Every visitor can, including guests. Admin does not force a language on anyone** — admin controls only the site default and which locales are available at all.

This mirrors the 3-tier pattern the codebase already uses for `theme`:

| Actor | Can change | Stored in | Scope |
|-------|-----------|-----------|-------|
| **Guest** | Own display language | `ferum_locale` cookie + URL subpath | That browser |
| **Member** | Own display language | `user_preferences.locale` | Follows the account across devices |
| **Admin** | Site *default* + enabled locale roster | `site_config` | Applies only to visitors who have not chosen |

One important deviation from `theme`: guest theme is stored in `localStorage` and applied client-side by `ferum-preload.js`. **Locale cannot work that way** — it is baked into the SSR output, so it must travel with the request. Guest locale is therefore a **cookie**, not `localStorage`.

### UI surfaces

1. **Language switcher in `partials/nav.html`** — next to the existing `[data-ferum-theme-btn]` toggle. Visible to guests and members alike. For guests it sets the cookie and redirects; for members it also persists via the preferences API. Rendered only when more than one locale is enabled.
2. **Account → Preferences tab** — a control alongside the existing theme / font-size / layout buttons in `app/account.html`, following the same `data-account-action` pattern.
3. **Admin → Languages** — a dedicated management page, see Step 7. (`site_config` holds `default_locale` and `enabled_locales`, but they are edited there, not on the generic Settings page.)
4. **Setup wizard** — a language picker on `/setup`. An admin installing in Vietnam should not have to complete first-run in English and then go hunting for the setting.

### API changes

`PUT /api/users/me/preferences` gains a `locale` field. The existing handler validates against hardcoded arrays (`valid_themes`, `valid_font_sizes`, `valid_layouts`); `locale` must instead validate against the **admin-enabled roster**, so disabling a locale in admin immediately stops new users from selecting it:

```rust
let locale = body.locale.unwrap_or(existing.locale);
if !enabled_locales.contains(&locale) {
    return Err(AppError::unprocessable("locale_not_enabled").into());
}
```

Note this is a coded error, per Step 1 — not prose.

### Precedence conflict: preference vs. URL

A subtlety the negotiation order has to resolve. A logged-in Vietnamese user clicks a shared link to `/forum/t/slug` (unprefixed = the default locale). Do they get English or Vietnamese?

Resolution: **unprefixed means "no explicit choice"**, so their preference applies and they get Vietnamese. To make the default locale still explicitly addressable, `/en/forum/t/slug` is accepted as a valid prefix even though `en` is unprefixed in canonical URLs for SEO. So:

- `/forum/t/slug` → negotiate (preference → cookie → `Accept-Language` → default)
- `/en/forum/t/slug` → forced English, canonicalized to `/forum/t/slug`
- `/vi/forum/t/slug` → forced Vietnamese

Guests hitting an unprefixed URL that negotiates to a non-default locale get a **redirect** to the canonical subpath, never a differing render at the same URL — this is what keeps the Nginx guest cache correct (Step 5).

### Routing

Add `locale` to `UserPreferences` (which already carries `theme`, `font_size`, `layout`) and a migration for the column.

Rather than duplicating every route under each locale prefix, a middleware strips a recognized prefix and rewrites the URI before the router matches:

```rust
// /vi/forum/t/slug  →  Locale::parse("vi") + URI rewritten to /forum/t/slug
// /forum/t/slug     →  negotiated locale, URI untouched
```

This keeps `public_routes.rs` / `admin_routes.rs` / `mod_routes.rs` completely unchanged. The default locale stays unprefixed so existing URLs and inbound links never break.

Link generation must then be locale-aware — a `url()` Tera helper that prefixes non-default locales, so templates keep writing logical paths.

### SEO

`base.html` emits `hreflang` alternates for each supported locale plus `x-default`, and `<html lang="{{ locale }}">` replaces the hardcoded `lang="en"`. The sitemap generator emits one entry per locale with `xhtml:link` alternates.

---

## Step 5 — Formatting, emails, and the cache trap

### Guest SSR cache — do this before anything ships

The Nginx guest page cache uses `Vary: Cookie` with a 30s TTL. Once SSR output depends on locale, that cache **will serve Vietnamese pages to English guests**. Because the subpath strategy puts the locale in the URL, the cache key is already correct for prefixed URLs — but the unprefixed default-locale URL must not be reachable in a non-default locale via cookie alone. Enforce: cookie/`Accept-Language` negotiation triggers a *redirect* to the canonical subpath for guests, never a differing render at the same URL.

### Email locale

`ForumJob::SendEmailVerification` / `SendPasswordResetEmail` / `SendNotificationEmail` carry no locale, and the worker runs detached from the request. The recipient's locale **must be resolved at enqueue time** and carried in the job payload — it cannot be recovered in `inline_runner.rs`. Add a `locale: Locale` field to each variant.

Note this is the recipient's locale, not the actor's: a Vietnamese user replying to an English user's thread must produce an English notification email.

### Dates and numbers

Replace `{{ x | date(format="%b %d, %Y") }}` with a locale-aware `localdate` filter, and make `group_thousands()` take the locale's separator. Both are registered on the per-locale Tera instance, so call sites need no locale argument.

---

## Step 6 — String extraction

Catalogs live as Fluent `.ftl` files, keyed by domain:

```
locales/
  en/  common.ftl  forum.ftl  auth.ftl  errors.ftl  admin.ftl  email.ftl
  vi/  …
```

**Extensibility must be designed in here, not retrofitted.** Themes and plugins ship their own strings:

```
frontend/themes/{slug}/locales/{locale}/theme.ftl    # merged with theme-chain precedence
plugins/{slug}/locales/{locale}/plugin.ftl           # namespaced by plugin slug
```

A theme that overrides `thread.html` will introduce strings the core catalog has never seen. Bundle merge order must mirror the template resolution order — child theme wins over parent, parent over core.

Order of extraction: `errors.ftl` (falls out of Step 1) → public templates (392) → client JS → admin/mod (567, extracted but English-only per the locked decision).

### Client-side strings

The ~3.5k lines of `ferum-page-*.js` and the Svelte widgets carry strings like `'Network error. Please try again.'`. SSR bootstraps a `window.Ferum.i18n` dictionary containing only the keys that page needs, injected in `base.html`. Widgets read from it rather than embedding literals. This avoids shipping every locale's full catalog to the browser.

---

## Step 7 — Admin language management (`/admin/languages`)

Two `site_config` keys are not enough for a self-hosted product. A self-hoster needs to add a language and reword forum vocabulary without editing files on disk and redeploying — the same reason `/admin/themes` accepts uploads rather than requiring a git commit.

Slots into the admin nav next to Themes and Plugins. Gated by a new permission key **`admin.languages`** (`min_trust: new`, admin-only), consistent with the RBAC table rather than overloading `admin.config`.

### Catalog precedence — the same chain, a third time

The override model reuses the exact resolution shape already proven twice in this codebase (theme inheritance, locale fallback):

```
DB override  →  theme/plugin .ftl  →  core locale .ftl  →  default locale  →  raw key
```

DB overrides are **sparse** — only strings the admin actually changed. That keeps upgrade drift small and makes it possible to flag "this string was overridden, but the source text has since changed upstream" in the UI. Invalidation reuses the `reload()` + Arc-swap pattern from `RolePermissionCache` and `TeraEngine`.

### Tiers, in priority order

**Tier 1 — Locale roster** *(ships with Phase 3b)*

- Enable / disable locales, set the site default
- Translation coverage per locale (`% of core keys resolved`), computed by walking the default-locale catalog — cheap, and it is the number an admin actually wants
- Warn before disabling a locale that users have selected

**Tier 2 — Language pack upload** *(Phase 7)*

A `.zip` of `.ftl` files, mirroring the theme upload flow. Reuse the existing archive validation path — size cap, path-traversal rejection on the locale tag, required-file check. Meaningfully cheaper than it looks because that code already exists for themes.

**Tier 3 — String override editor** *(Phase 8, optional)*

Search across keys and values, filter by `missing` / `overridden` / `stale`, edit inline. This is what lets an admin turn "Thread" into "Topic" site-wide, which is among the most-requested self-hosting capabilities.

### Security note

Admin-authored translation strings are **user-controlled content rendered into every page**, so:

- Translated strings must **never** be piped through `| safe` in any template. Tera autoescapes by default; the risk is a well-meaning `| safe` added later to allow bold text in a message. Add this to `docs/security-audit-checklist.md`.
- Fluent placeables in an override must be validated against the source string's argument set at save time — an override referencing an undefined variable should be rejected on write, not fail at render.
- Overrides are an audit-logged admin action (`AuditLog`), like other `admin.*` mutations.

### Routes

```
GET    /admin/languages                    page
GET    /api/admin/languages                roster + coverage
PATCH  /api/admin/languages/:locale        enable/disable, set default
POST   /api/admin/languages/upload         language pack (.zip)
DELETE /api/admin/languages/:locale        remove an installed pack
GET    /api/admin/languages/:locale/strings   search/filter (Tier 3)
PUT    /api/admin/languages/:locale/strings/:key   override (Tier 3)
DELETE /api/admin/languages/:locale/strings/:key   revert to source (Tier 3)
```

---

## Explicitly Out of Scope

**User-generated content is not translated.** Threads, posts, category names, and tags stay in whatever language they were authored. This single decision removes the largest and least tractable chunk of work.

Mitigations that cost almost nothing:

- Optional `language` column on categories, so a community can run per-language sections
- `lang` attribute on rendered post content for screen readers and search engines
- Machine translation deferred to an optional Tier 2 plugin using the existing plugin system

**Postgres FTS stays on `'simple'`.** It applies no stemming, which is the correct language-neutral default. Per-language `regconfig` would require a `language` column on threads and a dynamic trigger — real work for modest gain. Communities needing strong multilingual search should enable the Meilisearch toggle, which handles this natively.

---

## Phasing and Effort

| Phase | Scope | Effort |
|-------|-------|--------|
| 1 | Step 1 — error codes (independently valuable; ship alone) | ~1 week |
| 2 | Steps 2–3 — Translator port, Fluent adapter, per-locale Tera | ~1 week |
| 3 | Step 4 — negotiation, subpath routing, `hreflang`, cache fix | ~4 days |
| 3b | Step 4 — switcher UI (nav + account), setup-wizard picker, preferences API, admin locale roster (Step 7 Tier 1) | ~3 days |
| 4 | Step 5 — formatting, email locale plumbing | ~3 days |
| 5 | Step 6 — public templates + client JS extraction | ~1.5 weeks |
| 6 | Admin/mod extraction, English-only catalogs | ~4 days |
| 7 | Step 7 Tier 2 — language pack upload (reuses theme upload path) | ~3 days |
| 8 | Step 7 Tier 3 — string override editor (optional) | ~1 week |

**Total ≈ 5.5–6 weeks** for a fully translated public surface with admin structured for later, plus language pack upload. Tier 3 is a further week and can be deferred indefinitely — it adds no capability that editing `.ftl` files does not already provide, only convenience.

Phase 1 is shippable on its own and improves the API contract whether or not i18n proceeds. Phases 2–4 deliver no visible change until Phase 5 lands — worth sequencing so a single locale (`en`) runs through the full pipeline before a second one is added, proving the plumbing without translation cost.

---

## Open Risks

| Risk | Mitigation |
|------|------------|
| Guest cache serves wrong language | Canonical-subpath redirect for guests; never render differing content at one URL |
| Theme authors' strings fall outside catalogs | Theme-scoped `.ftl` with chain precedence, designed in at Step 6 |
| Translation drift as features land | CI check that every `t(k=…)` key resolves in the default locale |
| Email sent in actor's locale, not recipient's | Resolve locale at enqueue time; assert in the notification subscriber |
| Locale from URL used unvalidated | `Locale::parse` returns `Option`; no raw string ever reaches bundle lookup |

---

## Implementation Notes (2026-07-21)

Where the built code deviates from the plan above, and why.

### `Locale` is an owned `String`, not `&'static str`

The Step 2 sketch used `&'static str`, which would have made Step 7 Tier 2
(language pack upload) impossible without a recompile — a locale that arrives at
runtime cannot become `'static`. Tags are 2–10 bytes, so the owned form costs
nothing measurable. Parsing also canonicalizes casing, so a cookie saying `EN-us`
and a URL saying `en-US` cannot produce two `Locale`s that split the template cache.

### `Translator` takes `TransArg`, not `FluentArgs`

The plan's port signature named `FluentArgs`, which would have made
`ferum-application` depend on `fluent-bundle` — the very crate the port exists to
hide. `TransArg` is a small owned enum living in `ferum-domain` (it has to be in
the domain because `AppError` carries translation arguments).

Its `Int`/`Float`/`Str` split is load-bearing rather than cosmetic: Fluent selects
plural forms from the argument's *type*, so passing a stringified count silently
collapses every plural rule to its catch-all arm in every language.

### `base_chain` vs `fallback_chain`

Two chains, because matching and resolving need opposite behaviour at the end:

- `fallback_chain` ends at the default locale — correct for *resolving a message*,
  so an untranslated string shows in English rather than as a raw key.
- `base_chain` stops at the base language — correct for *negotiation*.

The first implementation used one chain for both. A unit test caught the result: a
German-only `Accept-Language` "matched" English through the fallback tail, so the
server reported it had negotiated German and then served English, ignoring every
lower-priority language in the header. Regression test:
`uninstalled_language_does_not_silently_match_the_default`.

### Errors: a new `AppError::Invalid` variant rather than reusing `UnprocessableEntity`

`UnprocessableEntity(String)` still exists and still carries prose, because ~18
sites hold genuinely dynamic text — mostly `validator::ValidationErrors::to_string()`
and plugin-package diagnostics that interpolate a parse error. Those are a separate
problem needing per-field validator code mapping, and most are admin-facing plugin
debugging that arguably should not be translated at all.

`AppError::Invalid { code, args }` is the translatable path. Adding a variant rather
than changing the existing one meant no existing match site had to churn.

**`human_message` was deleted.** English error text now lives *only* in
`locales/en/errors.ftl`, satisfying the no-duplication rule — but that makes the
middleware load-bearing, since without it users would see `thread locked` instead of
a sentence. Two things guard this: the middleware is attached at the router root, and
`error_catalog.rs` fails the build if any emitted code lacks an entry.

### Error translation happens in middleware, not `IntoResponse`

`IntoResponse` sees neither `AppState` nor request extensions, so it cannot reach a
translator or a locale. The only in-place alternatives were a process-global
translator or a task-local — both rejected (this codebase has no global state, and
the testing plan lists that as a property worth keeping).

Instead `AppError::into_response` attaches an `ErrorPayload { code, args }` to the
*response* extensions, and `translate_errors` — which has both the locale and the
translator — rewrites the `message` field on the way out. Responses without a payload
pass through unbuffered.

Two consequences worth knowing:

- `Content-Length` is removed when the body is rewritten; a translated message is a
  different length and a stale header truncates the response.
- HTML error pages are produced by `error_page_layer`, which sits *inside* this layer
  and replaces the response wholesale, so **HTML error pages are not yet translated**.
  JSON API errors are.

### Layer ordering in the router

`negotiate_locale` → `translate_errors` → `auth` → … → handler.

`translate_errors` must be inside `negotiate_locale` (it reads the `Locale`
extension off the request before delegating) and outside `auth` and the handlers, so
it also catches errors from layers that never reach a handler — 401s, CSRF
rejections, rate limits.

### Per-locale Tera confirmed correct

`TeraEngine::render` dispatches the render onto `spawn_blocking`. This retroactively
confirms rejecting the `tokio::task_local!` approach: a task-local would not survive
that hop, so the locale had to be bound into the instance. Admin, mod, and the plugin
review partial explicitly render in the default locale, matching the locked decision
to ship those English-only.

`t()` output is escaped by Tera like any other value, and
`translated_output_is_html_escaped` fails if a `| safe` is ever introduced on a
translated string — catalogs become admin-editable in Step 7 Tier 3, which would turn
that into an XSS vector.

### Still English-only, by design

`thousands` still hardcodes its separator and templates still call
`date(format="%b %d, %Y")`. Both are now trivial to localize — the per-locale
instance can register locale-aware versions — but doing it properly needs a
separator/format table per locale, and half-doing it is worse than leaving it
visibly untouched.

---

## Implementation Notes — Part 2 (2026-07-21)

Completes the user-facing half: preference storage, the switcher, the admin
roster, localized emails, and a real second language.

### `user_preferences.locale` is nullable on purpose

`NULL` means "never chosen", which is genuinely different from having chosen the
default language. Only the former follows the site default when an admin changes
it, and only the former falls through to `Accept-Language`. A user who
deliberately picks English stays on English even if the site later switches its
default to Vietnamese.

This three-way distinction reaches the API too: on
`PUT /api/users/me/preferences`, an absent `locale` field leaves the choice
alone, while an explicit `null` clears it. `Option<Option<Locale>>` with
`#[serde(default)]` alone cannot express that — serde collapses both to `None` —
so the field goes through a `deserialize_with` that maps a present-but-null value
to `Some(None)`.

### The preference is stored in the DB but carried by a cookie

The negotiator needs the locale on *every* request, before any handler runs. The
two obvious ways to get a signed-in user's preference there were a per-request
database read or a new JWT claim; the first is a query on every page render, the
second means re-minting tokens whenever someone changes a setting.

Instead the DB row stays the durable record and the `ferum_locale` cookie is a
cache of it, refreshed at login and on every preference write. Guests use the
same cookie with no account behind it, so one negotiation path serves both.

Unlike the theme preference — which lives in `localStorage` and is applied
client-side by `ferum-preload.js` — the locale **cannot** work that way: it is
baked into the server-rendered HTML, so it has to travel with the request.

### `PUT /api/locale` exists for guests

Guests are ~70% of the audience and have no account to hold a preference, so
language switching cannot be gated behind `users/me`. The endpoint sets the
cookie for anyone, and additionally persists to the account when one is present —
which lets the nav switcher post to it unconditionally.

### Locale validation is against the live roster, not a constant

`theme`, `font_size`, and `layout` validate against hardcoded arrays. `locale`
cannot: the valid set is whatever is installed and enabled right now. Validating
against `translator.available_locales()` means removing a language pack
immediately stops anyone selecting it, with no allowlist to keep in sync.

### `RequestLocale` carries the canonical path

`hreflang` alternates must point at *this page* in every other language, but by
render time the original URI is gone — the prefix has already been stripped for
routing. `negotiate_locale` therefore captures the post-strip path and passes it
alongside the locale, and page handlers hand the pair to `render_with_theme_in`.

This is why 19 page handlers gained an `Extension(req_locale)` parameter. Without
it `t()` would resolve against the default catalog on every public page,
regardless of what the visitor chose — the feature would appear wired but do
nothing.

### Emails resolve the recipient's locale at enqueue time

`ForumJob::Send*Email` now carries a `Locale`. The worker runs detached from any
request, so there is no locale to consult when it executes — and the request
locale would be the *actor's* anyway. A Vietnamese member replying to an English
member's thread must not send them a Vietnamese email.

Registration is the exception that proves it: a brand-new account has no stored
preference, so the request locale is the only evidence of what language the
person reads, and the verification email is the first thing they receive.

`JobExecutor` holds the translator as an `Option`. When absent it degrades to
emitting catalog keys rather than failing to send — a verification link the user
can still click beats a silent failure that locks them out of a new account.

### Admin → Languages reports coverage, not just a list

Coverage counts only the keys a locale translates *itself*. A key inherited from
the default locale still renders in the wrong language, so counting it would
report a fully-translated site that isn't — which is precisely the number an
admin is looking at this page to learn.

Two guards the API enforces rather than trusting the UI: the last enabled locale
cannot be disabled, and neither can the site default. Either would leave visitors
with no language to be served.

Gated by a new `admin.languages` permission rather than `admin.config`, so a
community translator can be given language access without also receiving SMTP
credentials and the registration switch.

### Vietnamese ships as a real second locale

`locales/vi/` is deliberately **partial** — it translates the common UI and the
errors users actually hit, and inherits English for the rest. That is the state
every real translation is in most of the time, and
`second_locale_translates_and_inherits` asserts it works: `vi` strings resolve,
untranslated keys fall back to English rather than rendering raw keys, and
arguments still interpolate.

Vietnamese has no plural inflection, so its `thread-replies` selector collapses
to one form. The selector structure is kept anyway so the catalog stays
structurally aligned with the source, and a test guards against someone adding an
`[one]` arm that would never be selected.

### Still outstanding

- The bulk of template strings (~950) and client JS (~3.5k lines) are still
  hardcoded English. `t()` works and the pattern is demonstrated in
  `partials/thread_card.html`, `partials/nav.html`, and `app/account.html`.
- HTML error pages remain untranslated (`error_page_layer` sits inside the
  translation middleware and replaces the response wholesale).
- `thousands` and `date(format=…)` are still locale-independent.
- Step 7 Tier 2 (language-pack upload) and Tier 3 (string override editor).

---

## Implementation Notes — Part 3 (2026-07-21)

Bulk string extraction, client-side i18n, locale-aware formatting, and the audit.

### Extraction was scripted, and the script corrupted templates twice

Roughly 1,450 replacement sites across 54 files is not hand work, so it was done
by a rewriting script. Two bugs in it are worth recording because both produced
**templates that still parsed and still passed every existing test**:

1. **Empty mask sentinel.** The script masks `<script>`, `<style>` and Tera
   regions before rewriting text, then restores them. The sentinel was written as
   a literal control character in the source file and did not survive the write,
   leaving the placeholder as a bare number — so the restore pass replaced the
   first *digits it found anywhere*, including digits inside real content.
2. **Guard ordering.** `base.html` contains a `{# … #}` comment whose prose
   mentions `<script>`. Masking `<script>…</script>` *before* comments made the
   non-greedy match run from that word inside the comment to the next real
   `</script>` — deleting the comment tail and the `ferum-api.js` tag with it.

Both were caught only by diffing the tag structure against the pre-change
snapshot. The lesson is in the tests below: a Tera parse test proves a template
*compiles*, not that it still contains what it did before.

### `template_integrity.rs` exists because of that

Five structural invariants, deliberately not string assertions:

- `<script>` open/close counts balance in every template
- `base.html` and `admin/base.html` still load their required scripts by name
- no leftover extraction sentinels
- no translated string is ever piped through `| safe`
- no template still uses `date(format=…)`

The script-tag test was verified against the real defect: removing
`ferum-api.js` from `base.html` makes it fail, so it is not vacuous.

### Keys are derived from the English source text

`ui-no-threads-yet`, not `ui-forum-empty-state-1`. A derived key reads as a gloss
of the string, so if one ever leaks into the UI — a missing catalog entry renders
the key — it is self-describing rather than opaque. It also makes the catalog
diffable against the templates by eye.

Namespaces: `ui-` public theme, `adm-` admin/mod, `js-` client-rendered,
`error-` error codes, `format-`/`month-short-` formatting, `rev-` the
`ferum-review` theme.

### Code was extracted as if it were copy, and had to be reverted

The heuristic pulled in 27 strings that name things rather than say them:
permission keys (`category.create`), audit-log actions (`user.banned`), paths
(`assets/`), filenames (`theme.json`), camelCase identifiers
(`createNotification`). Translating any of those breaks what it names — a
permission key rendered in Vietnamese no longer matches the permission. A second
pass reverts anything matching those shapes back to a literal.

### Sentences split across markup were merged

Auto-extraction happily turns `Visit a category and click <strong>Watch</strong>
to follow it` into three fragments. That cannot be translated: word order differs
by language, and the template fixes the order the pieces are reassembled in.
Seven such sentences were merged into whole messages, restyling the markup where
needed — `<a>Sign in</a> to join the conversation` became a single link reading
"Sign in to join the conversation", which is also a bigger tap target and so
closer to NF-UX-02.

### Dates and numbers are catalog-driven, not strftime

`date(format="%b %d, %Y")` cannot be localized: chrono's `%b` is always English
and the field order is fixed in the pattern even though languages disagree about
it. The `localdate` filter supplies only the numbers; the month name
(`month-short-3`) and the arrangement (`format-date = { $month } { $day }, { $year }`)
both come from the catalog. Vietnamese reorders to `{ $day } { $month }, { $year }`
and uses `.` as its thousands separator purely by editing its own `.ftl`.

Time fields are pre-padded strings because Fluent formats a bare number as `9`,
never `09`.

### Client strings ship as a scoped dictionary

Only the `js-` namespace is serialised into the page, via
`<meta name="ferum-i18n">` — a meta attribute rather than an inline `<script>`
because the CSP forbids inline scripts. Shipping the whole catalog would put
admin copy a visitor can never see into every page, growing without bound as the
site is translated. `Ferum.t(key, args)` reads it and interpolates `{ $name }`
placeables client-side, falling back to the key.

Admin pages load the same shared `ferum-utils.js`, so `admin/base.html` emits the
dictionary too — otherwise every client-rendered admin message would show a raw
key.

### Themes own their catalogs

The audit turned up `frontend/themes/ferum-review/`, a second installed theme
with 69 strings that the first extraction pass never touched. Its strings now
live in `themes/ferum-review/locales/en/theme.ftl`, and `startup.rs` discovers
every `themes/*/locales` directory and appends it to the translator roots *after*
core — so a theme can both add its own copy and override a core string, matching
the precedence direction the theme template chain already resolves in.

This is the extensibility path the plan called for; it is now exercised by a real
theme rather than only by a unit test.

### Audit result

Every `.html` under `frontend/` was checked. The files with no `t()` calls are
all correct:

| File | Why |
|------|-----|
| `partials/pagination.html` | Only calls a macro; the strings live in `macros.html` |
| `partials/notif_bell.html`, `errors/base.html` | Pure markup, no prose |
| `static/errors/*_static.html` | Served **when Tera is unavailable** — they cannot call `t()` by definition, and are English-only on purpose |
| `client-widgets/node_modules/**` | Vendored |

`examples/themes/*` are reference material, not loaded at runtime, and were left
alone deliberately.

### What is still English

- ~44 public and ~30 admin nodes mix text with interpolation
  (`Welcome back, {{ user }}`). Those need a Fluent placeable each, which is a
  judgement call per string rather than a mechanical rewrite; the extractor
  reports them instead of guessing.
- Page `{% block title %}` contents, for the same reason — nearly all interpolate
  `site.name`.
- Admin/mod copy is extracted but ships English-only, per the locked decision.
  `render_admin` renders in the default locale; switching that one call is the
  entire remaining code change.
- Vietnamese covers navigation, forum, auth, account, and catalog — roughly the
  first two screens a reader meets. The rest inherits English through the
  fallback chain, which `second_locale_translates_and_inherits` asserts is safe.

---

## Implementation Notes — Part 4 (2026-07-22)

Two runtime defects that no test caught, and the tests added so they cannot recur.

### The whole site rendered raw keys, because the catalog directory was never found

`LOCALES_DIR` defaults to `./locales`, but the app is run from `backend/` (as
CLAUDE.md instructs), so it resolved to `backend/locales` — which does not exist.
The fallback path was *also* `./locales`, so it resolved to the same missing
directory. `FluentTranslator` then did exactly what it was designed to do:
degrade gracefully and render each key as its own text. The navigation read
`ui-home`, `ui-categories`, `ui-products`.

Every test passed throughout, because the test harness builds its own translator
from an explicit repo-root path. Nothing connected "the catalog the tests load"
to "the catalog the server loads."

Three changes:

1. `LOCALES_DIR=../locales` added to `.env` and `.env.example`, matching how
   `THEMES_DIR` already handles the same cwd split.
2. The fallback now tries `./locales` *and* `../locales` before giving up, and
   **hard-fails startup** if neither exists.
3. Startup also fails when the default locale loads **zero messages**.

That third check reverses an earlier decision recorded in Part 1, which said a
missing catalog "is not fatal — degrades to rendering keys, which is ugly but
still serves pages." That reasoning was wrong. A forum whose navigation reads
`ui-home` is not degraded, it is broken, and the graceful degradation is what
hid the fault. First-party templates already fail closed; catalogs now match.

### `/vi/…` returned 404 — `Router::layer` runs *after* routing

`negotiate_locale` strips the locale prefix from the URI so every route can be
declared once. It was attached with `Router::layer`, which in axum applies
middleware to each **endpoint** — so route matching has already happened by the
time it runs. `/vi/forum` matched nothing, fell through to the 404 fallback, and
the prefix was stripped far too late to matter.

The fix nests the fully-routed router behind an outer `Router` as a
`fallback_service`, with the two locale layers on the outer one. The outer router
has no routes, so every request lands on its fallback: the middleware runs, the
URI is rewritten, and only then does the inner router match.

This also puts `translate_errors` genuinely outermost, so it now catches errors
from layers it previously sat inside.

### `template_keys.rs`

The test that would have caught the first defect, and the answer to "check every
page": it scans every `t(k="…")` in every template under `frontend/` and asserts
each key resolves in the real catalogs — core plus every theme's own. 500+ keys
across 54 files, with a floor assertion so a broken scanner cannot make it
vacuous.

It skips concatenated keys (`t(k="language-name-" ~ alt)`), which have no single
literal to check; `every_installed_locale_names_itself` covers those instead.

A companion test reports catalog keys no template references, so the catalogs do
not silently accrete dead strings. It only warns — `js-*` keys are used from
JavaScript and `error-*` from the error middleware, so absence from markup is
expected there.

### UI fixes found in the same pass

- **Review theme's topbar search vanished.** A pre-existing
  `body:has(.fr-hero) … { display: none }` removed it from the flex flow on
  masthead pages, leaving a visible hole between brand and icons. The two search
  fields serve different jobs — page CTA versus persistent chrome — so both now
  show, with the bar's field toned down instead of hidden.
- **Review theme's mobile drawer was see-through.** `.fr-sidebar-left` is one
  element in two roles: a rail on the canvas at ≥lg, and a drawer panel below it.
  The theme set `background: transparent`, right for the rail and wrong for the
  panel. Now scoped: transparent as a rail, solid inside `#mobileSidebar`.
- **Language switcher moved left of the theme toggle**, per request — both are
  display preferences and language is the more consequential.
- **Admin/mod brand unified with the public topbar.** Admin used
  `.adm-brand-logo` (30px, `--bs-primary`, hardcoded "Fe") while the forum used
  `.fr-brand-mark` (32px, `--ferum-primary`, site initial), so the badge changed
  size and colour when crossing into admin. Both now use `.fr-brand-mark` and
  derive the initial from the site name.
