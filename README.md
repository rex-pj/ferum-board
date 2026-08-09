# Ferum Board

Ferum Board is a self-hosted forum server. It renders thread discussions, a product
catalogue with reviews, moderation tooling and an admin panel as server-rendered HTML,
and exposes the same data over a JSON API. It runs as a **single process** — one Rust
binary serves HTML, the API, static files and theme assets on one port. No Node.js
process is required at runtime.

It is aimed at operators who want to run a discussion community on their own hardware:
everything is in your PostgreSQL database and your filesystem or object store. Redis,
S3/GCS and Meilisearch are optional — the app starts without them and uses in-process
fallbacks.

## Tech stack

Versions come from `backend/Cargo.toml` and `frontend/client-widgets/package.json`.

| Layer | Technology |
| --- | --- |
| Language | Rust, edition 2021, Cargo workspace |
| HTTP | Axum 0.8 on Tokio 1.52 |
| Database | PostgreSQL, via Sea-ORM 2 (`sqlx-postgres`) |
| Migrations | `sea-orm-migration` 2, in `backend/migration` |
| HTML | Tera 1 (Jinja2 syntax), server-rendered |
| Auth | `jsonwebtoken` 9 (httpOnly cookie) + `bcrypt` 0.15 |
| Markdown | `pulldown-cmark` 0.13, sanitised with `ammonia` 4 |
| i18n | Fluent (`fluent-bundle` 0.16), catalogs in `locales/` |
| Email | Lettre 0.11 over SMTP |
| Interactive widgets | Svelte 5.55 web components, built with Vite 8 |
| CSS / icons | Bootstrap 5.3.3, Font Awesome Free 6.7.2, Alpine.js (all vendored under `frontend/static/`) |
| Cache / rate limit | Redis 0.27 client — optional, in-memory fallback |
| Object storage | `aws-sdk-s3` 1 (feature `s3`) or a GCS XML-API client (feature `gcs`) — optional, database fallback |
| Search | PostgreSQL full-text search; Meilisearch 0.28 optional (feature `meilisearch`) |
| Plugin scripting | `boa_engine` 0.19 (feature `script_plugins`, on by default) |

## Requirements

| | Version | Notes |
| --- | --- | --- |
| Rust | current stable | No minimum is declared in the repo (`rust-version` is unset and there is no `rust-toolchain.toml`). CI builds with `dtolnay/rust-toolchain@stable`. |
| PostgreSQL | 16 | CI runs `postgres:16-alpine`. The `unaccent` extension is used if available; without it search still works but stops folding diacritics. |
| Node.js | 20.19+ or 22.12+ | **Only** needed to rebuild the Svelte widgets. Vite 8 sets this floor. The compiled bundle is committed, so running the app needs no Node. |
| C toolchain | platform default | On Linux, `backend/.cargo/config.toml` pins the linker to `clang` with `-fuse-ld=mold`, so both must be installed (`apt install clang mold`) or that block must be commented out. On Windows the linker is `rust-lld`, which ships with rustup. |

Redis, MinIO/S3, Google Cloud Storage and Meilisearch are all optional and none is
needed for local development.

## Quickstart

### 1. Clone

```bash
git clone https://github.com/rex-pj/ferum-board.git
cd ferum-board
```

### 2. Create the database

```bash
createdb ferum_board_db
```

Or with `psql`:

```bash
psql -U postgres -c "CREATE DATABASE ferum_board_db;"
```

### 3. Write the config file

```bash
cd backend
cp .env.example .env
```

Then edit `backend/.env`. Two values must be changed before the app will do anything
useful:

- `DATABASE_URL` — point it at the database you just created.
- `JWT_SECRET` — minimum 32 characters. Generate one with `openssl rand -hex 32`.

The path variables (`THEMES_DIR`, `ADMIN_TEMPLATES_DIR`, `STATIC_DIR`, `LOCALES_DIR`,
`PLUGINS_DIR`) are already set to `../frontend/...` and `../locales`, which resolve
correctly **only when the process runs from `backend/`**.

### 4. Run

```bash
# from backend/
cargo run
```

Migrations are applied automatically at startup, followed by an idempotent seed of the
rows the app cannot boot without (system roles, permissions, site config defaults, the
built-in theme, the catalogue taxonomy). There is no separate migrate or seed step.

The first build compiles ~390 crates and takes several minutes.

### 5. Complete first-run setup

Open <http://localhost:5173>. Until setup is finished every HTML request redirects to
`/setup`. Create the admin account there. Ticking "seed example data" on that page
populates demo users, categories, threads and catalogue rows.

To skip the wizard (for scripted installs), set all three of `SETUP_ADMIN_USERNAME`,
`SETUP_ADMIN_EMAIL` and `SETUP_ADMIN_PASSWORD` in `.env`; the admin is created on
startup instead.

### 6. Rebuild the Svelte widgets — only if you change them

```bash
cd frontend/client-widgets
npm install        # first time only
npm run build      # writes frontend/static/js/ferum-widgets.iife.js
```

Commit the rebuilt bundle. Nobody else needs Node to run the app.

## Configuration

All configuration is environment variables, loaded by `backend/crates/ferum-web/src/config.rs`
via `dotenvy` + the `config` crate. `backend/.env.example` lists every variable the app
reads and is the reference; the table below summarises it.

### Required

| Variable | Default | Description |
| --- | --- | --- |
| `DATABASE_URL` | — | PostgreSQL connection string. |
| `APP_URL` | — | Public base URL. Used for links in email and as an allowed CSRF origin. |
| `JWT_SECRET` | — | Signing key for access and refresh tokens. Minimum 32 characters. |
| `FROM_EMAIL` | — | Sender address on outgoing mail. |

`JWT_EXPIRY_SECONDS` (3600) and `REFRESH_TOKEN_EXPIRY_DAYS` (7) have defaults but are
written out in `.env.example` because they are worth a deliberate choice.

### Server and paths

| Variable | Default | Description |
| --- | --- | --- |
| `BIND_ADDR` | `0.0.0.0` | Interface to bind. |
| `PORT` | `5173` | Listen port. |
| `CORS_ORIGINS` | `http://localhost:5173` | Comma-separated. Also forms the CSRF origin allowlist together with `APP_URL`. Empty or `*` disables credentialed CORS and logs a warning. |
| `TRUSTED_PROXY_COUNT` | `0` | Number of reverse proxies in front. `0` ignores `X-Forwarded-For`. Set to `1` behind a single Nginx/Cloudflare, or per-IP rate limiting keys on the proxy's address. |
| `MAX_UPLOAD_SIZE_MB` | `5` | Per-request body ceiling, raised automatically to fit a plugin package. |
| `THEMES_DIR` | `./frontend/themes` | |
| `ADMIN_TEMPLATES_DIR` | `./frontend/templates` | Admin, mod and setup templates. |
| `STATIC_DIR` | `./frontend/static` | Served at `/static/`. |
| `LOCALES_DIR` | `./locales` | One subdirectory per locale. Startup fails if no catalog is found. |
| `PLUGINS_DIR` | `./plugins` | Where uploaded plugins are extracted. |

### Database pool

| Variable | Default | Description |
| --- | --- | --- |
| `DB_MAX_CONNECTIONS` | `40` | Per pool. A single page render can hold 4–6 connections because handlers run their COUNT and data queries concurrently. |
| `DB_MIN_CONNECTIONS` | `2` | |
| `DATABASE_READ_URL` | unset | Read replica. Unset means all reads go to the primary. |

Every application connection carries `statement_timeout=30s` and
`idle_in_transaction_session_timeout=60s`, appended to the URL automatically. Supply
your own `options=` in `DATABASE_URL` to override.

### Optional capabilities

Presence of the variable is the toggle. Three of these **also require a cargo feature**;
without it the variable is ignored and a warning is logged at startup.

| Variable | Cargo feature | Fallback when unset | Enables |
| --- | --- | --- | --- |
| `REDIS_URL` | — | in-memory | Shared cache and rate-limit counters across instances. Background jobs are `tokio::spawn` either way. |
| `S3_ENDPOINT` + `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, `S3_REGION` | **`s3`** | database `stored_files` table | S3-compatible object storage (AWS, MinIO, R2). |
| `GCS_BUCKET` (+ `GCS_PREFIX`, credentials) | **`gcs`** | database `stored_files` table | Google Cloud Storage. **Wins over `S3_ENDPOINT`** when both are set. |
| `CDN_BASE_URL` | — | same-origin `/files/{key}` | Serve uploads from a CDN. |
| `MEILISEARCH_URL` (+ `MEILISEARCH_KEY`, `MEILISEARCH_INDEX`, `MEILISEARCH_PRODUCT_INDEX`) | **`meilisearch`** | PostgreSQL FTS | Typo-tolerant, faceted search. |
| `RATE_LIMIT_ENABLED` | — | `true` in code, `false` in `.env.example` | Per-IP rate limiting. |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS` | — | mail disabled | Outgoing email. These four are seeded into `site_config` on first startup and are editable afterwards at `/admin/settings`. |

GCS credentials, highest precedence first: `GCS_CREDENTIALS_JSON`,
`GCS_CREDENTIALS_FILE`, `GOOGLE_APPLICATION_CREDENTIALS`. Set none of them on
GCE/GKE/Cloud Run and the metadata server supplies the token. The bucket must be
readable by `allUsers` — see the note in `.env.example` for the exact IAM binding and
why it must be `legacyObjectReader`.

### Behaviour and logging

| Variable | Default | Description |
| --- | --- | --- |
| `DEDUP_VIEW_COUNTS` | `false` | Count each logged-in viewer once per thread per day. Guests are not counted when on. |
| `PLUGIN_HOOK_TIMEOUT_MS` | `500` | A `before_*` hook exceeding this is killed and the request is allowed through. |
| `PLUGIN_CIRCUIT_THRESHOLD` | `10` | Consecutive failures before a plugin's circuit opens. |
| `LOG_LEVEL` | `ferum_web=info,…` | Fallback filter. **`RUST_LOG` overrides it entirely** when set. |
| `LOG_FORMAT` | `pretty` | `pretty` or `json`. |
| `LOG_DIR` | unset | Also write a daily-rotating file. Unset means stdout only. |
| `SLOW_QUERY_MS` | `500` | Warn on repository queries slower than this. |

## Directory structure

```
ferum-board/
├── backend/                      Cargo workspace; run cargo from here
│   ├── .env.example              every variable the app reads
│   ├── Cargo.toml                [workspace.dependencies] — the only place versions live
│   ├── crates/
│   │   ├── ferum-domain/         models + repository traits; no I/O, no frameworks
│   │   ├── ferum-application/    use cases, port traits, permission checks
│   │   ├── ferum-infrastructure/ Sea-ORM repos, storage/search/cache/plugin adapters
│   │   └── ferum-web/            Axum handlers, routers, middleware, Tera engine
│   ├── migration/                Sea-ORM migrations — DDL only, no seed rows
│   └── tests/                    support, domain, application, infrastructure, web
├── frontend/
│   ├── themes/default/           built-in theme: templates/ + assets/ + theme.json
│   ├── templates/                admin/, mod/, partials/, setup.html (not themeable)
│   ├── static/                   css/, js/, fonts/, errors/ — served at /static/
│   └── client-widgets/           Svelte sources; build output goes to static/js/
├── locales/                      Fluent catalogs, one directory per locale (en, vi)
├── examples/
│   ├── plugins/                  7 working example plugins + their .fpkg archives
│   └── themes/                   3 example themes + their .zip archives
├── scripts/                      regen-entities.*, package-examples.*
├── docs/                         see docs/README.md
└── plugins/                      runtime extract directory (gitignored)
```

Dependencies flow inward: `ferum-web` → `ferum-application` → `ferum-domain`, and
`ferum-infrastructure` → `ferum-application`/`ferum-domain`. The domain crate builds
with neither Axum nor Sea-ORM enabled, so the boundary is enforced by the compiler
rather than by convention.

## Commands

All cargo commands run from `backend/`.

```bash
cargo run                                              # dev server on :5173
cargo build --release                                  # production binary: target/release/ferum-board
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo machete                                          # unused dependencies
```

**`--workspace` is mandatory for `test` and `clippy`.** `default-members` is set to
`crates/ferum-web` so that a bare `cargo run`/`cargo build` builds the server. That
same setting narrows what `cargo test` selects: without `--workspace` it runs one test
and prints `ok` while running none of the real suites, and `cargo clippy` lints one
crate and exits 0.

Standalone migration CLI, when you need to run migrations outside the server:

```bash
cargo run -p migration --features cli -- up
cargo run -p migration --features cli -- down
cargo run -p migration --features cli -- fresh      # drop everything and re-apply
```

`--features cli` is required — the bin target is behind `required-features` so the
server binary does not link an argument parser.

Other scripts, run from the repository root:

```bash
pwsh scripts/regen-entities.ps1          # regenerate Sea-ORM entities from the schema
pwsh scripts/package-examples.ps1        # rebuild examples/**.fpkg and **.zip
pwsh scripts/package-examples.ps1 -Check # verify the archives match their sources
```

`scripts/regen-entities.sh` and `scripts/package-examples.sh` are the bash equivalents.
Do not run `package-examples` concurrently with `cargo test --workspace` — it rewrites
`examples/plugins/*/bundle.js`, which the domain test suite reads.

### Cargo features

Two axes with opposite defaults. `s3`/`gcs`/`meilisearch` need external infrastructure,
so they are opt-in. `script_plugins`/`bulk_seed` work standalone, so they are opt-out.

| Feature | Default | Compiles in |
| --- | --- | --- |
| `script_plugins` | on | `boa_engine` — the Tier 2 JavaScript plugin runtime |
| `bulk_seed` | on | the setup wizard's example dataset |
| `s3` | off | `S3StorageService` |
| `gcs` | off | `GcsStorageService` |
| `meilisearch` | off | `MeilisearchService` |

```bash
cargo build --release --features s3            # production with object storage
cargo build --release --no-default-features    # lean: no JS engine, no demo data
```

Both defaults degrade honestly when off: Script-tier plugins are reported as
unsupported, and the seed checkbox fails with `seed_data_unavailable` rather than
finishing setup with an empty forum.

### Build profiles

| Profile | LTO | Codegen units | Use |
| --- | --- | --- | --- |
| `dev` | — | 256 | Everyday work. Dependencies build without debug info. |
| `release` | fat | 1 | What you ship. The link step alone takes over ten minutes. |
| `release-fast` | thin | 16 | Optimised but quick, for profiling. Not for shipping. |

## Developer notes

### Layer rules

- Handlers do HTTP marshalling and template context assembly only. Business rules live
  in use cases.
- Use cases depend on repository traits (`ferum-domain/src/repositories/`) and port
  traits (`ferum-application/src/ports.rs`), never on Sea-ORM, Axum or a concrete
  client. `startup.rs` picks the adapter.
- Every state-changing use case calls `PermissionChecker`
  (`ferum-application/src/permission.rs`) before it mutates anything. Middleware
  resolves identity; it does not authorise.
- Dependency versions live in `[workspace.dependencies]` in `backend/Cargo.toml`. Never
  write a bare version string in a member manifest — two crates naming different
  versions silently produces two builds of the same library.

### Adding a route

1. Add the route to the right file in `backend/crates/ferum-web/src/routings/`:
   `public_routes.rs` (public pages, auth, member API), `admin_routes.rs` (`/admin/*`,
   `/api/admin/*`, `/api/setup/*`, `/files/*`) or `mod_routes.rs` (`/mod/*`,
   `/api/mod/*`).
2. Add the handler under `handlers/pages/`, `handlers/api/`, `handlers/admin/` or
   `handlers/moderation/`.
3. For a page, add a context struct in `view_models/page_context.rs` if no existing one
   fits.

### Adding a template

Public and member pages go in `frontend/themes/default/templates/`; admin, mod and
setup pages go in `frontend/templates/`. Render public pages with
`render_with_theme(&state, &active_theme, "page.html", &ctx)` — never
`state.tera.render()` directly, or the theme inheritance chain and the plugin slot
context are skipped.

Every user-facing string in a public or member template must go through
`{{ t(k="ui-...") }}`, including `placeholder`, `title`, `aria-label` and `alt`. Add
the key to **every** locale under `locales/`, not just `en`. Namespaces are not
interchangeable: `ui-*` for server-rendered public text, `js-*` for the dictionary
exposed to client JavaScript, `adm-*` for admin panels, `error-*` for API error text.
See `docs/i18n.md`.

### Tests

Test code lives in `backend/tests/`, as five workspace members rather than `#[cfg(test)]`
modules: `support` (fixtures and mocks), `domain`, `application`, `infrastructure`
(needs a live PostgreSQL) and `web`. `cargo test --workspace` runs all of them.

The infrastructure suite provisions a throwaway database per test from
`TEST_DATABASE_URL`, falling back to `DATABASE_URL`. It refuses to run against a
non-local host.

CI additionally runs the whole suite twice, under `TZ=UTC` and `TZ=Asia/Kathmandu`,
because the second is a non-whole-hour offset that catches date code which truncates
instead of converting.

## Deployment

The application terminates no TLS and runs plain HTTP. Put a reverse proxy in front.

### Docker Compose

```bash
cp backend/.env.example .env.prod
# Edit .env.prod. Set at minimum: JWT_SECRET, APP_URL, FROM_EMAIL,
# DB_PASSWORD, S3_ACCESS_KEY, S3_SECRET_KEY, and the SMTP_* block.
docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --build
```

`--env-file` is required. A service's `env_file:` only supplies variables *inside* the
container; the `${...}` substitutions in the Compose file are resolved by Compose
itself, which reads only `--env-file` or a `.env` beside it. Without the flag,
`DB_PASSWORD` and the MinIO credentials expand to empty strings and neither Postgres
nor MinIO will start.

`DATABASE_URL`, `REDIS_URL` and `S3_ENDPOINT` are set by the Compose file to the
service addresses and do not belong in `.env.prod` — a value there would be
overridden anyway.

The stack is the app plus PostgreSQL 16, Redis 7, MinIO and Nginx. `Dockerfile` builds
with `--features s3` so the MinIO service is actually used; change the `FEATURES`
build arg to `gcs`, or to an empty string for database storage. `nginx.conf` holds the
TLS server block — replace `forum.example.com` and point `ssl_certificate` at real
files before starting.

Two settings must agree with the proxy, and both are already set in the Compose file:
`TRUSTED_PROXY_COUNT=1`, so rate limiting sees the client IP rather than Nginx's, and
`APP_URL` set to the public HTTPS URL, so CSRF origin checks and email links are
correct.

**These four files were written for this repository but have not been executed** —
there is no Docker daemon in the environment they were authored in, so
`docker build` and `docker compose config` are unverified. Treat the first deploy as
a shakedown.

### Without Docker

```bash
cd backend
cargo build --release --features s3     # or --no-default-features for the lean build
```

The binary is `backend/target/release/ferum-board`. It needs the environment variables
above, and `THEMES_DIR`/`ADMIN_TEMPLATES_DIR`/`STATIC_DIR`/`LOCALES_DIR`/`PLUGINS_DIR`
must resolve from its working directory. Migrations run on startup, so a deploy is
"replace the binary and restart".

`SIGTERM` triggers a graceful shutdown that drains in-flight requests.

### Health endpoints

| Path | Checks | Use for |
| --- | --- | --- |
| `/health`, `/health/live` | nothing — 200 while the process answers | liveness / restart decisions |
| `/health/ready` | database, cache, upload readability | readiness / traffic decisions |

Only an unreachable database makes `/health/ready` return 503. Unreadable uploads are
reported in the body but do not take the instance out of rotation.

## Troubleshooting

**`cargo test` prints `ok` after one test.** You omitted `--workspace`. See the
Commands section.

**`cargo test --workspace` stalls and never finishes.** `tests/infrastructure` needs a
reachable local PostgreSQL — it creates and drops a database per test — and without
one it does not fail fast. Start the database, or run the suites that do not need it:

```bash
cd backend
cargo test -p ferum-domain-tests -p ferum-application-tests -p ferum-web-tests
```

**Startup aborts with a translation-catalog error.** `LOCALES_DIR` does not resolve to
a directory containing `.ftl` files. It is relative to the process working directory,
so `cargo run` must be started from `backend/`. The app tries `./locales` and
`../locales` as fallbacks and logs a WARN naming the one it used; if none exists it
refuses to boot rather than serve pages reading `ui-home` / `ui-categories`.

**Blank page, or CSS and JS 404.** Same cause for `STATIC_DIR` / `THEMES_DIR` /
`ADMIN_TEMPLATES_DIR`, which fall back to `./frontend/*` with a WARN. Check that
warning, and the working directory, first. A template that fails to *parse* is treated
differently — it aborts startup, because retrying the fallback path would only parse
the same bad file again.

**Link step fails on Linux with `cannot find -fuse-ld=mold`.** Install the linker
toolchain (`sudo apt-get install clang mold`) or comment out the
`[target.x86_64-unknown-linux-gnu]` block in `backend/.cargo/config.toml`.

**Uploads succeed but images are broken.** Under S3 or GCS the bucket must be publicly
readable — the browser fetches `public_url` directly and no request reaches the app.
For GCS, startup probes this and logs a WARN with the exact `gcloud` command; the
result is also reported by `/health/ready` as `"uploads"`.

**`S3_ENDPOINT`/`GCS_BUCKET` is set but files still go to the database.** The build
lacks the `s3`/`gcs` cargo feature. A warning naming the missing feature is logged at
startup.

**Migrations fail on an old database.** Databases created before the migration series
was consolidated carry stale `seaql_migrations` rows and cannot migrate forward. Drop
and recreate.

**Redirected to `/setup` on every page.** First-run setup has not been completed. If it
was, `setup_complete` is derived from whether an admin account exists.

**A plugin's widget does not appear.** The custom element name is derived by the server
as `ferum-slot-{plugin-slug}-{slot-name}`; the plugin's `bundle.js` must call
`customElements.define` with exactly that string. An unknown element renders as an
empty inline box with no error.

## Documentation

- [docs/README.md](docs/README.md) — index of the remaining documentation
- [docs/plugin-system/plugin-developer-guide.md](docs/plugin-system/plugin-developer-guide.md) — writing a plugin
- [docs/plugin-system/technical-design.md](docs/plugin-system/technical-design.md) — how the plugin runtime works
- [docs/db-entities-and-testing.md](docs/db-entities-and-testing.md) — entity regeneration and the DB test harness
- [docs/i18n.md](docs/i18n.md) — locale catalogs and key namespaces
- [docs/security-audit-checklist.md](docs/security-audit-checklist.md) — pre-release audit procedure

## License

[Ferum Board Attribution License](LICENSE). Free to use, modify and redistribute,
provided every deployment displays "Powered by Ferum Board" in a visible location on
each page served. A commercial license removing the attribution requirement is
available: trungle.it@gmail.com
