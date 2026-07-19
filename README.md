# Ferum Board

> A modern, self-hosted forum built on Rust + Axum + Tera — Reddit-level experience with full data ownership.

![Rust](https://img.shields.io/badge/Rust-1.78+-orange?logo=rust)
![Axum](https://img.shields.io/badge/Axum-0.8-blue?logo=rust)
![PostgreSQL](https://img.shields.io/badge/PostgreSQL-16-blue?logo=postgresql)
![License](https://img.shields.io/badge/License-Attribution-green)

---

## Why Ferum Board?

Running a community means your data, your rules. Ferum Board is built for teams and communities that want a modern, thread-based discussion platform without giving up control — self-hosted, open source, and designed to run comfortably on a single VPS or scale out when needed.

- **Full data ownership** — your server, your database, your files
- **Modern experience** — mobile-first UI, dark mode, real-time notifications, Markdown editor
- **Built on Rust** — low memory footprint, high throughput, no runtime crashes
- **No vendor lock-in** — every infrastructure component (cache, storage, search) has an open-source self-hosted option and a safe fallback

---

## Features

### Content & Discussion

- **Thread-based discussions** with Markdown editor and live preview
- **Nested replies**, quote reply, soft-delete with edit window (24h)
- **Best Answer** — mark the post that solved the thread
- **Reactions** — Like, Helpful, Insightful, Funny per post
- **Thread tags** — free-form labels, filter feed by tag
- **Thread thumbnails** — image per thread for visual feeds
- **Bookmarks** — save threads for later

### Discovery & Search

- **Home feed** — newest, hottest, unanswered, solved filters
- **Full-text search** — PostgreSQL FTS (Phase 1) or Meilisearch (Phase 2)
- **Forum index** — category overview with thread counts and last activity
- **Tag-based feeds** — browse all threads by tag

### Users & Profiles

- **Registration** with email verification
- **Trust levels** (New → Basic → Member → Regular → Leader) — anti-spam gate for new accounts
- **Avatar + cover image** upload (content-addressed storage, deduplicates automatically)
- **Public profile page** — post history, stats, trust badge
- **User preferences** — dark/light/auto theme, font size, notification opt-in per event
- **Watched & muted categories** — per-user feed personalization

### Notifications

- **In-app notification inbox** with unread count
- **Server-Sent Events (SSE)** real-time push — no WebSocket overhead
- **Email notifications** — per-event opt-in (reply, mention, reaction, best answer)
- **Automatic polling fallback** if SSE connection drops

### Moderation

- **RBAC** — roles and permissions stored in DB, fully configurable from the admin UI without code changes
- **Category-scoped moderators** — assign a moderator to specific categories only
- **Report queue** — report posts/threads, resolve or dismiss per category
- **Warn / temp ban / permanent ban** with reason history
- **Pin, lock, move threads** — scoped to assigned categories
- **Audit log** — every mod/admin action is recorded and searchable

### Administration

- **Dashboard** — DAU, MAU, new threads, new users, pending reports
- **Role manager** — create custom roles (e.g. `vip`, `supporter`), assign permission matrix per role
- **Category CRUD** — 2-level hierarchy, view policy (public/members_only/staff_only), post policy
- **Site config** — name, logo, primary color, SMTP, registration toggle, keyword blacklist
- **Webhooks** — subscribe any ForumEvent to an external URL with HMAC-signed payloads
- **First-run setup wizard** — create the initial admin account before first use

### Plugins

- **3-tier plugin system** — Tier 1 (Manifest: declarative webhooks), Tier 2 (Script: sandboxed JS via boa_engine), Tier 3 (Service: sidecar process — planned)
- **Before-hooks** — plugins can inspect and block requests (post create, thread create, user register…)
- **After-events** — fire-and-forget event dispatch to all active plugins
- **UI slots** — plugins can inject Web Components into designated page slots
- **Circuit breaker** — a misbehaving plugin is isolated; the forum always stays up

### Developer / Operations

- **Capability toggles via env vars** — Redis, S3, Meilisearch, read replica are each optional; the system starts and works without them using safe fallbacks
- **Content-addressed file storage (CAS)** — SHA-256 keyed, ref-counted; backed by S3 (production) or the database (development)
- **XML sitemap** auto-generated for all public pages
- **Graceful shutdown** — drains in-flight requests before stopping
- **Structured JSON logging** with request IDs

---

## Screenshots

> Screenshots coming soon. If you have deployed Ferum Board, feel free to open a PR with your own!

---

## Tech Stack

| Layer               | Technology                                                        |
| ------------------- | ----------------------------------------------------------------- |
| API server          | **Rust** + **Axum 0.8** (async, Tokio)                            |
| Database            | **PostgreSQL 16** (FTS, JSONB, triggers)                          |
| ORM / migrations    | **Sea-ORM 1.0**                                                   |
| HTML rendering      | **Tera** (Jinja2 syntax, server-rendered)                         |
| Interactive islands | **Svelte 5** Web Components (compiled to `ferum-widgets.iife.js`) |
| Admin / mod UI      | **HTMX** + **Alpine.js** + vanilla JS                             |
| UI framework        | **Bootstrap 5.3**                                                 |
| Auth                | JWT (httpOnly cookie) + bcrypt                                    |
| Email               | Lettre 0.11 (SMTP)                                                |
| Plugin scripts      | **boa_engine** (pure Rust JS sandbox, no V8/Deno)                 |
| Cache / rate limit  | Redis 7 (optional; in-memory fallback)                            |
| File storage        | S3-compatible / MinIO (optional; PostgreSQL fallback)             |
| Search              | Meilisearch (optional; PostgreSQL FTS fallback)                   |

---

## Quick Start — Development

### Prerequisites

- Rust 1.78+
- PostgreSQL 16
- Node.js 20+ (only needed when modifying Svelte Web Components)

```bash
git clone https://github.com/your-username/ferum-board.git
cd ferum-board
```

**Run the app (single process — serves HTML, API, and static files):**

```bash
cd backend
cp .env.example .env   # edit DATABASE_URL — THEMES_DIR/STATIC_DIR already set to ../frontend/*
cargo run              # app at http://localhost:5173
```

The app is at `http://localhost:5173`. On first run, navigate to `/setup` to create the initial admin account.

> No Redis, S3, or Meilisearch required in development. The app uses in-memory fallbacks automatically.

**Rebuild Svelte Web Components (only when modifying widgets in `frontend/client-widgets/`):**

```bash
cd frontend/client-widgets
npm install   # first time only
npm run build # emits frontend/static/js/ferum-widgets.iife.js — commit the result
```

---

## Quick Start — Production (Docker Compose)

```bash
git clone https://github.com/your-username/ferum-board.git
cd ferum-board

cp .env.example .env.prod
# Edit .env.prod — set DATABASE_URL, JWT_SECRET, SMTP_*, REDIS_URL, S3_*

docker compose -f docker-compose.prod.yml up -d
```

The stack includes: PostgreSQL, Redis, MinIO (S3-compatible), Nginx (TLS termination), Rust/Axum/Tera API (serves HTML + JSON).

### TLS

Point your domain at the server, then configure `nginx.conf` with your Let's Encrypt certificate path. The app itself runs plain HTTP behind Nginx — TLS is terminated at the proxy layer.

---

## Configuration

All configuration is via environment variables. No config files required.

### Required

| Variable       | Description                                        |
| -------------- | -------------------------------------------------- |
| `DATABASE_URL` | PostgreSQL connection string                       |
| `APP_URL`      | Public base URL (e.g. `https://forum.example.com`) |
| `JWT_SECRET`   | 256-bit random hex string                          |
| `FROM_EMAIL`   | Sender address for outgoing email                  |

### Optional (with fallbacks)

| Variable                                           | Default when absent         | What it enables                            |
| -------------------------------------------------- | --------------------------- | ------------------------------------------ |
| `REDIS_URL`                                        | In-memory (single instance) | Async jobs, Redis rate limit, shared cache |
| `S3_ENDPOINT`                                      | PostgreSQL table (dev only) | Scalable file storage                      |
| `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`      | —                           | Required when `S3_ENDPOINT` is set         |
| `CDN_BASE_URL`                                     | App URL for file links      | CDN edge for media                         |
| `MEILISEARCH_URL`                                  | PostgreSQL FTS              | Typo-tolerant, faceted search              |
| `DATABASE_READ_URL`                                | Primary for all reads       | Read replica routing                       |
| `RATE_LIMIT_ENABLED`                               | `true`                      | Set `false` for local dev                  |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS` | —                           | Outgoing email                             |

See [CLAUDE.md](CLAUDE.md) for the full reference.

---

## Permission Model

Ferum Board uses **full RBAC** — roles and permissions are stored in the database and configurable at runtime via the admin UI:

- **System roles** — `admin`, `moderator`, `member` (cannot be deleted)
- **Custom roles** — create `vip`, `supporter`, etc. via `/admin/roles`
- **Permission matrix** — assign any of 26+ permission keys to any role
- **Trust level gate** — separate anti-spam layer (New / Basic / Member / Regular / Leader)
- **Category-scoped roles** — a moderator can be assigned to specific categories only

---

## Deployment Profiles

| Profile            | Infrastructure                    | Use case                     |
| ------------------ | --------------------------------- | ---------------------------- |
| **A — Dev**        | PostgreSQL only                   | Local development, no Docker |
| **B — Staging**    | PostgreSQL + Redis + MinIO        | Single VPS, small production |
| **C — Production** | All services + CDN + read replica | High-traffic production      |

The app detects which services are configured and switches adapters automatically. Missing services degrade gracefully with logged warnings — the server never refuses to start.

---

## Attribution

All deployments must display the following notice in a visible location (e.g. site footer):

> Powered by Ferum Board.

To use Ferum Board without the attribution notice, purchase a commercial license: **trungle.it@gmail.com**

See [LICENSE](LICENSE) for full terms.

---

## Contributing

Contributions are welcome. Please open an issue before submitting a pull request for large changes.

- Follow the architecture rules in [CLAUDE.md](CLAUDE.md)
- Run `cargo clippy` and `cargo test` before pushing
- If you changed Svelte widgets, run `npm run build` in `frontend/client-widgets/` and commit the updated bundle

---

## License

[Ferum Board Attribution License](LICENSE) — free to use with attribution. Commercial license available.
