# Plugin System — Technical Design

**Version:** 1.0.0  
**Status:** Implemented — Tier 1 (Manifest) + Tier 2 (Script/boa_engine) done; Tier 3 (Service) planned  
**Last Updated:** 2026-06-23

---

## Table of Contents

1. [Overview](#1-overview)
2. [System Architecture](#2-system-architecture)
3. [Plugin Tiers](#3-plugin-tiers)
4. [Plugin Lifecycle](#4-plugin-lifecycle)
5. [Hook Dispatch Flow](#5-hook-dispatch-flow)
6. [Frontend Slot System](#6-frontend-slot-system)
7. [Database Design](#7-database-design)
8. [Security Model](#8-security-model)
9. [API Reference](#9-api-reference)
10. [Configuration Reference](#10-configuration-reference)

---

## 1. Overview

The Ferum Board plugin system extends the forum without modifying core code. It is designed around six non-negotiable principles:

| Principle | Meaning |
|---|---|
| **Forum First** | Forum runs 100% correctly even when all plugins are disabled |
| **Fail-Safe** | A crashing plugin never brings down the forum |
| **Least Privilege** | A plugin only receives the exact data it declares it needs |
| **Explicit Consent** | Admin must understand and grant each capability before activation |
| **Owned Boundaries** | Plugins own their own data, schema, and migration history |
| **Observable** | Every plugin action is logged and auditable |

### Relationship to Existing System

The current webhook system handles ~60% of external integration needs. The plugin system builds on top of it rather than replacing it:

```
Webhooks ──► Tier 1 Manifest Plugins  (declarative, config-driven)
                    +
             Tier 2 Script Plugins    (sandboxed JS, before-hooks)
                    +
             Tier 3 Service Plugins   (sidecar process, full capability)
```

---

## 2. System Architecture

### 2.1 High-Level Overview

```mermaid
graph TB
    subgraph CLIENT["Client Layer"]
        Browser["Browser / Tera SSR (Axum)"]
        PluginSlot["&lt;PluginSlot&gt; Component"]
        WebComp["Web Components\n(plugin UI)"]
    end

    subgraph CORE["Ferum Core (Rust / Axum)"]
        Handler["Web Handlers\nferum-web"]
        UseCase["Use Cases\nferum-application"]
        Domain["Domain Models\nferum-domain"]
        Infra["Repositories\nferum-infrastructure"]
    end

    subgraph RUNTIME["Plugin Runtime Layer"]
        Registry["PluginRegistry\n(implements PluginHookRuntime +\nPluginUiRuntime + PluginLifecycle)"]
        ManifestLoader["Manifest\nLoader"]
        ScriptRuntime["Script Runtime\n(boa_engine — pure Rust JS)"]
        ServiceOrch["Service\nOrchestrator"]
    end

    subgraph PLUGINS["Plugin Processes"]
        P1["Tier 1\nManifest Plugin\n(config only)"]
        P2["Tier 2\nScript Plugin\n(JS isolate)"]
        P3["Tier 3\nService Plugin\n(sidecar)"]
    end

    subgraph DATA["Data Layer"]
        PG[("PostgreSQL\npublic schema")]
        PluginPG[("PostgreSQL\nplugin_{slug} schema")]
    end

    Browser --> Handler
    Browser --> PluginSlot
    PluginSlot --> WebComp
    Handler --> UseCase
    UseCase --> Registry
    UseCase --> Domain
    UseCase --> Infra
    Infra --> PG
    Registry --> ManifestLoader --> P1
    Registry --> ScriptRuntime --> P2
    Registry --> ServiceOrch --> P3
    P3 --> PluginPG
    P3 -.->|"Plugin Query API\n/api/plugins/_internal/*"| Handler
    WebComp -.->|"Plugin API\n/api/plugins/{slug}/*"| P3
```

### 2.2 Dependency Flow

```mermaid
graph LR
    W["ferum-web\n(handlers, routing)"]
    A["ferum-application\n(use cases, ports)"]
    D["ferum-domain\n(models, events)"]
    I["ferum-infrastructure\n(Sea-ORM, Redis, S3)"]
    PR["Plugin Runtime\n(new layer in infrastructure)"]

    W --> A
    A --> D
    I --> A
    I --> D
    PR --> A
    PR --> D

    style PR fill:#f0e6ff,stroke:#9b59b6
```

The plugin runtime sits inside `ferum-infrastructure` and depends only on `ferum-application` port traits and `ferum-domain` types — the same constraint as any other infrastructure adapter.

---

## 3. Plugin Tiers

### 3.1 Capability Matrix

```mermaid
graph LR
    subgraph T1["Tier 1 — Manifest"]
        T1A["✓ Register webhooks"]
        T1B["✓ Listen to ForumEvents"]
        T1C["✓ Admin config panel"]
        T1D["✗ Run any code"]
        T1E["✗ Block requests"]
        T1F["✗ Own database"]
        T1G["✗ Inject UI"]
    end

    subgraph T2["Tier 2 — Script"]
        T2A["✓ Everything in Tier 1"]
        T2B["✓ Run JS in boa_engine sandbox\n(pure Rust, no V8/Deno)"]
        T2C["✓ Block requests (before-hook)"]
        T2D["✓ Outbound HTTP (allowlisted)"]
        T2E["✓ Isolated key-value cache"]
        T2F["✓ Inject UI via server-side slots\n(static HTML + client bundle.js)"]
        T2G["✗ Own database schema"]
    end

    subgraph T3["Tier 3 — Service"]
        T3A["✓ Everything in Tier 2"]
        T3B["✓ Run as separate process"]
        T3C["✓ Own PostgreSQL schema"]
        T3D["✓ Add API endpoints"]
        T3E["✓ Inject UI (Web Components)"]
        T3F["✓ Replace core providers\n(Search, Notifications)"]
    end

    style T1 fill:#e8f5e9,stroke:#2e7d32
    style T2 fill:#fff3e0,stroke:#ef6c00
    style T3 fill:#fce4ec,stroke:#c62828
```

### 3.2 Tier Selection Guide

```mermaid
flowchart TD
    Start([Plugin Requirement]) --> Q1{Needs to run\ncustom code?}
    Q1 -->|No| T1[Tier 1 — Manifest\nWebhooks, config forms,\nSlack/Discord integrations]
    Q1 -->|Yes| Q2{Needs to inject\nUI into pages?}
    Q2 -->|No| Q3{Needs its own\ndatabase?}
    Q3 -->|No| T2[Tier 2 — Script\nSpam filters, content transformers,\nauto-rules, UI banners/widgets]
    Q3 -->|Yes| T3
    Q2 -->|Yes| Q3
    T3[Tier 3 — Service\nChatbox, analytics dashboards,\nSSO providers — own DB schema]

    style T1 fill:#e8f5e9,stroke:#2e7d32
    style T2 fill:#fff3e0,stroke:#ef6c00
    style T3 fill:#fce4ec,stroke:#c62828
```

---

## 4. Plugin Lifecycle

### 4.1 State Machine

```mermaid
stateDiagram-v2
    [*] --> Validating : Admin uploads .fpkg

    Validating --> ReviewPending : Manifest valid
    Validating --> [*] : Validation failed\n(error shown to admin)

    ReviewPending --> Installing : Admin grants capabilities
    ReviewPending --> [*] : Admin rejects

    Installing --> Inactive : Files extracted\nDB migrations run\nPermissions seeded

    Inactive --> Configuring : Admin opens config
    Configuring --> Inactive : Config saved
    Inactive --> Active : Admin activates

    Active --> Inactive : Admin disables
    Active --> Error : Process crash / hook timeout storm
    Error --> Active : Admin force-restart\nor circuit resets (5min)
    Error --> Inactive : Admin disables

    Active --> Updating : Admin uploads new .fpkg
    Updating --> Active : New migrations run\nprocess restarted
    Updating --> Error : Migration failed\n(rollback attempted)

    Inactive --> Uninstalling : Admin confirms uninstall
    Active --> Uninstalling : Admin confirms uninstall
    Uninstalling --> [*] : Files deleted\nDB schema dropped\nPermissions removed
```

### 4.2 Install Sequence

```mermaid
sequenceDiagram
    participant Admin
    participant UI as Admin UI
    participant API as Forum API
    participant PR as PluginRegistry
    participant DB as PostgreSQL
    participant FS as Filesystem

    Admin->>UI: Upload chatbox.fpkg
    UI->>API: POST /api/admin/plugins (multipart)
    API->>PR: validate_package(bytes)
    PR->>PR: Parse plugin.toml\nCheck manifest schema\nCheck min_ferum version
    PR-->>API: ValidationResult { capabilities_requested }
    API-->>UI: 200 { id, capabilities_requested }

    UI->>Admin: Show capability review screen\n"This plugin will: run a process,\nown a DB schema, add API routes"
    Admin->>UI: Grant capabilities, click Install
    UI->>API: POST /api/admin/plugins/:id/install { granted_capabilities }

    API->>FS: Extract .fpkg to plugins/chatbox/
    API->>DB: INSERT INTO plugins (status='installing')
    API->>PR: run_migrations("chatbox", migrations/)
    PR->>DB: CREATE SCHEMA plugin_chatbox
    PR->>DB: Run 001_create_rooms.sql
    PR->>DB: Run 002_create_messages.sql
    PR->>DB: Record migration versions
    API->>DB: INSERT INTO permissions (plugin.chatbox.moderate)
    API->>DB: UPDATE plugins SET status='inactive'
    API-->>UI: 200 { status: 'inactive' }

    UI->>Admin: Show config form\n(rendered from config_schema)
    Admin->>UI: Fill config, click Save & Activate
    UI->>API: PATCH /api/admin/plugins/chatbox/config
    API->>API: Validate against config_schema
    API->>DB: UPDATE plugins SET config = {...}
    UI->>API: PATCH /api/admin/plugins/chatbox/status { active: true }
    API->>PR: activate("chatbox")
    PR->>PR: Spawn process (Tier 3)\nAwait heartbeat
    PR->>DB: UPDATE plugins SET status='active', activated_at=now()
    API-->>UI: 200 { status: 'active' }
    UI-->>Admin: Plugin is now active ✓
```

---

## 5. Hook Dispatch Flow

### 5.1 Before-Hook (Synchronous Gate)

```mermaid
sequenceDiagram
    participant C as HTTP Client
    participant H as Handler
    participant UC as UseCase
    participant PC as PermissionChecker
    participant PR as PluginRegistry
    participant P2 as Script Plugin (boa_engine)
    participant P3 as Service Plugin (sidecar)
    participant Repo as Repository

    C->>H: POST /api/threads/:id/posts
    H->>UC: create_post(actor, input)

    UC->>PC: check_create_post(actor, category_id)
    PC-->>UC: OK

    UC->>PR: dispatch_before_hook("before_post_create", ctx)

    loop For each active plugin (sorted by priority)
        alt Plugin is Script (Tier 2)
            PR->>P2: call isolate fn, timeout=500ms
            P2-->>PR: HookDecision
        else Plugin is Service (Tier 3)
            PR->>P3: POST /hook (Unix socket), timeout=500ms
            P3-->>PR: HookDecision
        end

        alt HookDecision = Deny
            PR-->>UC: Deny { reason, code }
            UC-->>H: AppError::blocked_by_plugin
            H-->>C: 403 { error: { code, message } }
        else HookDecision = Timeout / Error
            Note over PR: Log warning\nRecord failure\nContinue to next plugin\n(NEVER fail the request)
        else HookDecision = Allow
            Note over PR: Continue
        end
    end

    PR-->>UC: Allow

    UC->>Repo: post_repo.create(input)
    Repo-->>UC: Post

    UC->>UC: event_bus.publish(PostCreated)
    Note over UC: After-events are fire-and-forget\nPlugin errors here never affect response

    UC-->>H: Post
    H-->>C: 201 { data: post }
```

### 5.2 After-Event (Fire-and-Forget)

```mermaid
sequenceDiagram
    participant UC as UseCase
    participant EB as EventBus
    participant NS as NotificationSubscriber
    participant AS as AuditLogSubscriber
    participant WS as WebhookSubscriber
    participant PS as PluginSubscriber (new)
    participant P as Plugin

    UC->>EB: publish(PostCreated { ... })
    Note over EB: All subscribers called concurrently\nErrors are logged, never propagated

    par Existing subscribers
        EB->>NS: handle(PostCreated)
        EB->>AS: handle(PostCreated)
        EB->>WS: handle(PostCreated)
    and New plugin subscriber
        EB->>PS: handle(PostCreated)
        PS->>PS: Find plugins listening to "post.created"
        loop For each subscribed plugin
            PS->>P: dispatch_after_event (no timeout enforcement\nbut logged if slow)
            P-->>PS: OK / Error (logged, ignored)
        end
    end
```

### 5.3 Circuit Breaker Logic

```mermaid
flowchart TD
    Call["Plugin hook call"] --> CB{Circuit\nstate?}

    CB -->|Open| Skip["Skip plugin\nLog warning\nContinue request"]

    CB -->|Closed| Exec["Execute hook\n(with timeout)"]
    Exec --> Result{Result?}

    Result -->|Success| RecS["record_success()\nReset failure counter"]
    Result -->|Timeout / Error| RecF["record_failure()\nfailure_count += 1"]

    RecF --> Thresh{failure_count\n>= threshold?}
    Thresh -->|No| Continue["Continue request"]
    Thresh -->|Yes| Open["Set circuit_open = true\nLog: circuit opened\nNotify admin (in-app)"]
    Open --> Continue

    RecS --> Continue

    Skip --> Reset{5 min\nelapsed?}
    Reset -->|Yes| HalfOpen["Try one request\n(half-open probe)"]
    Reset -->|No| Continue
    HalfOpen --> Result
```

---

## 6. Frontend Slot System

### 6.1 Available Slots

```mermaid
graph TB
    subgraph PublicPages["Public Pages"]
        FH["forum.below_header\nShown below SiteHeader on all forum pages"]
        TH["thread.header\nBetween thread title and first post"]
        TB["thread.below_posts\nAfter last post — chatbox goes here"]
        UP["user.profile.section\nExtra section on public profile"]
        SA["search.results.after\nBelow search results list"]
    end

    subgraph AdminPages["Admin Pages"]
        DW["admin.dashboard.widget\nWidget card on /admin/dashboard"]
        SI["admin.sidebar.item\nMenu item in admin sidebar"]
        TA["admin.thread.actions\nExtra action in thread mod panel"]
    end

    style PublicPages fill:#e3f2fd,stroke:#1565c0
    style AdminPages fill:#f3e5f5,stroke:#6a1b9a
```

### 6.2 Slot Injection Architecture

```mermaid
sequenceDiagram
    participant SSR as Tera SSR (Axum handler)
    participant UC as UseCase / AppState
    participant API as Forum API
    participant DB as PostgreSQL
    participant Page as Tera template
    participant WC as Web Component (plugin)

    SSR->>UC: render page — calls plugin_ui.active_ui_slots()
    UC->>DB: SELECT from plugin_ui_slots WHERE is_active = true
    DB-->>UC: [{ slot_name, asset_url, custom_element_tag, props }]
    UC-->>SSR: plugin_slots injected into Tera context

    SSR->>Page: render with plugin_slots available in template
    Page->>Page: {% for slot in plugin_slots["thread.below_posts"] %}\n<script src="{{ slot.asset_url }}"></script>\n<{{ slot.tag }}></>{% endfor %}

    loop Browser parses custom element tags
        WC->>WC: connectedCallback()\nRead props from attributes
        WC->>API: GET /api/plugins/chatbox/rooms?threadId=...
        API-->>WC: rooms data
        WC->>WC: Render chat UI
    end
```

### 6.3 Web Component Loading

```mermaid
sequenceDiagram
    participant Browser
    participant TeraSSR as Tera SSR (Axum)
    participant PluginAsset as /plugins/chatbox/dist/thread-chatbox.js
    participant WCRegistry as customElements registry

    Browser->>TeraSSR: GET /forum/t/thread-slug
    TeraSSR-->>Browser: HTML with in <head>:\n<script src="/plugins/chatbox/dist/thread-chatbox.js"></script>

    Browser->>PluginAsset: GET /plugins/chatbox/dist/thread-chatbox.js
    PluginAsset-->>Browser: ES Module defining\nclass FerumChatboxThread\nextends HTMLElement

    Browser->>WCRegistry: customElements.define(\n  "ferum-chatbox-thread",\n  FerumChatboxThread\n)

    Note over Browser: When <PluginSlot> renders\n<ferum-chatbox-thread threadId="x">
    Browser->>Browser: Custom element\nconnectedCallback() fires
```

---

## 7. Database Design

### 7.1 Entity Relationship Diagram

```mermaid
erDiagram
    plugins {
        uuid id PK
        text slug UK
        text name
        text version
        plugin_tier tier
        plugin_status status
        jsonb manifest
        jsonb config
        jsonb granted_capabilities
        text install_path
        text db_schema_name
        int db_schema_version
        uuid installed_by FK
        timestamptz installed_at
        timestamptz updated_at
        timestamptz activated_at
        text error_message
        timestamptz last_seen_at
        int restart_count
        bool circuit_open
    }

    plugin_hooks {
        uuid id PK
        uuid plugin_id FK
        text hook_name
        int priority
        bool is_active
        int avg_ms
        timestamptz created_at
    }

    plugin_ui_slots {
        uuid id PK
        uuid plugin_id FK
        text slot_name
        text asset_url
        text custom_element_tag
        text_array props
        int load_order
        bool is_active
    }

    plugin_logs {
        uuid id PK
        uuid plugin_id FK
        text level
        text hook_name
        int duration_ms
        text message
        jsonb context
        timestamptz created_at
    }

    users {
        uuid id PK
        text username
    }

    plugins ||--o{ plugin_hooks : "registers"
    plugins ||--o{ plugin_ui_slots : "contributes"
    plugins ||--o{ plugin_logs : "emits"
    users ||--o{ plugins : "installs"
```

### 7.2 Plugin-Owned Schema Pattern

```mermaid
graph TB
    subgraph CoreSchema["PostgreSQL: public schema (forum core)"]
        users["users"]
        threads["threads"]
        posts["posts"]
        plugins_table["plugins"]
    end

    subgraph ChatboxSchema["PostgreSQL: plugin_chatbox schema"]
        rooms["rooms\n(thread_id — soft ref, no FK)"]
        messages["messages\n(room_id, user_id — soft refs)"]
        participants["participants"]
    end

    subgraph AnalyticsSchema["PostgreSQL: plugin_analytics schema"]
        pageviews["pageviews"]
        sessions["sessions"]
        events["events"]
    end

    plugins_table -->|"owns schema\n(db_schema_name)"| ChatboxSchema
    plugins_table -->|"owns schema\n(db_schema_name)"| AnalyticsSchema

    rooms -.->|"soft reference\n(no FOREIGN KEY)"| threads
    messages -.->|"soft reference"| users

    style CoreSchema fill:#e3f2fd,stroke:#1565c0
    style ChatboxSchema fill:#f3e5f5,stroke:#6a1b9a
    style AnalyticsSchema fill:#e8f5e9,stroke:#2e7d32
```

**Why no cross-schema FOREIGN KEYs?** Plugins must be loosely coupled to core. When a thread is deleted, the plugin cleans up via the `after_thread_deleted` event hook — not via cascade constraints. This allows the plugin to be installed and uninstalled without altering core schema.

### 7.3 SQL Schema

```sql
-- ─── Enums ────────────────────────────────────────────────────────────────────
CREATE TYPE plugin_tier AS ENUM ('manifest', 'script', 'service');
CREATE TYPE plugin_status AS ENUM (
    'installing', 'active', 'inactive', 'error', 'disabled', 'uninstalling'
);

-- ─── Plugins ──────────────────────────────────────────────────────────────────
CREATE TABLE plugins (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug                 TEXT UNIQUE NOT NULL,
    name                 TEXT NOT NULL,
    version              TEXT NOT NULL,
    tier                 plugin_tier NOT NULL,
    status               plugin_status NOT NULL DEFAULT 'installing',
    manifest             JSONB NOT NULL,
    config               JSONB NOT NULL DEFAULT '{}',
    granted_capabilities JSONB NOT NULL DEFAULT '{}',
    install_path         TEXT NOT NULL,
    db_schema_name       TEXT,
    db_schema_version    INT NOT NULL DEFAULT 0,
    installed_by         UUID REFERENCES users ON DELETE SET NULL,
    installed_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at         TIMESTAMPTZ,
    error_message        TEXT,
    last_seen_at         TIMESTAMPTZ,
    restart_count        INT NOT NULL DEFAULT 0,
    circuit_open         BOOLEAN NOT NULL DEFAULT false
);

-- ─── Plugin Hooks ─────────────────────────────────────────────────────────────
CREATE TABLE plugin_hooks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id   UUID NOT NULL REFERENCES plugins ON DELETE CASCADE,
    hook_name   TEXT NOT NULL,
    priority    INT NOT NULL DEFAULT 100,
    is_active   BOOLEAN NOT NULL DEFAULT true,
    avg_ms      INT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_plugin_hooks_dispatch
    ON plugin_hooks (hook_name, is_active, priority)
    WHERE is_active = true;

-- ─── Plugin UI Slots ──────────────────────────────────────────────────────────
CREATE TABLE plugin_ui_slots (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id           UUID NOT NULL REFERENCES plugins ON DELETE CASCADE,
    slot_name           TEXT NOT NULL,
    asset_url           TEXT NOT NULL,
    custom_element_tag  TEXT NOT NULL,
    props               TEXT[] NOT NULL DEFAULT '{}',
    load_order          INT NOT NULL DEFAULT 100,
    is_active           BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX idx_plugin_ui_slots_lookup
    ON plugin_ui_slots (slot_name, is_active, load_order)
    WHERE is_active = true;

-- ─── Plugin Logs (rolling; older entries pruned by a background job) ──────────
CREATE TABLE plugin_logs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id   UUID NOT NULL REFERENCES plugins ON DELETE CASCADE,
    level       TEXT NOT NULL CHECK (level IN ('trace','info','warn','error')),
    hook_name   TEXT,
    duration_ms INT,
    message     TEXT NOT NULL,
    context     JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

---

## 8. Security Model

### 8.1 Trust Boundaries

```mermaid
graph TB
    subgraph Trusted["Trusted — Full Access"]
        Core["Forum Core\n(Rust process)"]
        DB[("PostgreSQL\npublic schema")]
    end

    subgraph SemiTrusted["Semi-Trusted — Sandboxed"]
        V8["Script Plugin\n(boa_engine sandbox)"]
        QueryAPI["Plugin Query API\n/api/plugins/_internal/*"]
    end

    subgraph Untrusted["Untrusted — Process Boundary"]
        Sidecar["Service Plugin\n(separate process)"]
        PluginDB[("PostgreSQL\nplugin_{slug} schema")]
        External["External HTTP\n(allowlisted only)"]
    end

    Core <-->|"Direct\ncall"| DB
    Core -->|"Controlled\ncontext struct"| V8
    V8 -->|"Allowlisted\nHTTP only\n(not yet implemented)"| External
    Core -->|"Plugin Service\nToken (JWT)"| QueryAPI
    QueryAPI -->|"Read-only\npublic fields"| DB
    Sidecar -->|"Plugin Service\nToken"| QueryAPI
    Sidecar <-->|"Direct"| PluginDB
    Sidecar -->|"Allowlisted\nHTTP"| External

    style Trusted fill:#e8f5e9,stroke:#2e7d32
    style SemiTrusted fill:#fff3e0,stroke:#ef6c00
    style Untrusted fill:#fce4ec,stroke:#c62828
```

### 8.2 What a Plugin Can Never Access

| Resource | Tier 1 | Tier 2 | Tier 3 |
|---|:---:|:---:|:---:|
| `AppState` (raw) | ✗ | ✗ | ✗ |
| Core DB connection | ✗ | ✗ | ✗ |
| User password hashes | ✗ | ✗ | ✗ |
| JWT signing secret | ✗ | ✗ | ✗ |
| Email addresses (unconfirmed) | ✗ | ✗ | ✗ |
| Other plugins' config | ✗ | ✗ | ✗ |
| Forum filesystem (outside plugin dir) | ✗ | ✗ | ✗ |

### 8.3 Plugin Service Token

Tier 3 plugins authenticate against the Plugin Query API using a short-lived JWT:

```
Header:  { "alg": "HS256", "typ": "JWT" }
Payload: {
  "sub": "plugin:com.example.chatbox",
  "iss": "ferum-board",
  "iat": 1748900000,
  "exp": 1748900060,          // 60-second expiry, auto-refreshed
  "scope": [
    "read:threads",
    "read:users.public"
  ]
}
Signed with: PLUGIN_INTERNAL_SECRET (not the user JWT_SECRET)
```

### 8.4 Content Security Policy for Plugin UI

```
# Forum default CSP (in Nginx)
Content-Security-Policy:
  default-src 'self';
  script-src  'self' 'nonce-{request-nonce}';
  style-src   'self' 'unsafe-inline';

# Plugin Web Components are served from the same origin (/plugins/...)
# so they are covered by 'self'. No wildcard needed.
# Plugin components that call external APIs must do so server-side
# (through their sidecar), not from the browser.
```

---

## 9. API Reference

### 9.1 Admin Plugin Management

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/admin/plugins` | List all installed plugins |
| `POST` | `/api/admin/plugins/upload` | Upload `.fpkg` file (multipart) — returns temp path for review |
| `POST` | `/api/admin/plugins/install` | Confirm install after capability review |
| `GET` | `/api/admin/plugins/:slug` | Plugin detail + current status |
| `PATCH` | `/api/admin/plugins/:slug/config` | Update config values |
| `PATCH` | `/api/admin/plugins/:slug/status` | Activate / deactivate |
| `DELETE` | `/api/admin/plugins/:slug` | Uninstall plugin |
| `GET` | `/api/admin/plugins/:slug/logs` | Plugin execution logs (supports ?level=&since=) |

### 9.2 Plugin Query API (Internal — for plugins only)

Authenticated with Plugin Service Token header: `X-Ferum-Plugin-Token: <jwt>`

| Method | Path | Scope Required |
|---|---|---|
| `GET` | `/api/plugins/_internal/threads/:id` | `read:threads` |
| `GET` | `/api/plugins/_internal/threads` | `read:threads` |
| `GET` | `/api/plugins/_internal/users/:id` | `read:users.public` |
| `GET` | `/api/plugins/_internal/categories` | `read:categories` |
| `GET` | `/api/plugins/_internal/tags` | `read:tags` |

### 9.3 Plugin Public API (Tier 3 — mounted dynamically)

Routes declared in `api_routes` are forwarded by the forum to the plugin's sidecar:

```
Forum API Gateway
    │
    ├── /api/plugins/chatbox/*  ──► Chatbox sidecar (Unix socket proxy)
    ├── /api/plugins/analytics/* ──► Analytics sidecar
    └── ...
```

The forum acts as a reverse proxy: it validates the user's auth, injects `X-Ferum-User-Id` and `X-Ferum-User-Permissions` headers, then proxies to the plugin.

### 9.4 Plugin Slot API (Frontend hydration)

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/plugins/active-slots` | Returns all active UI slots for SSR hydration |

Response:
```json
{
  "thread.below_posts": [
    {
      "pluginId": "...",
      "pluginSlug": "com.example.chatbox",
      "assetUrl": "/plugins/com.example.chatbox/dist/thread-chatbox.js",
      "customElementTag": "ferum-chatbox-thread",
      "props": ["threadId", "categorySlug", "currentUser"],
      "loadOrder": 100
    }
  ]
}
```

---

## 10. Configuration Reference

### 10.1 Forum-Side Plugin Config (`config.rs`)

```rust
pub struct PluginConfig {
    /// Directory where extracted plugins are stored
    pub plugins_dir: PathBuf,                    // default: ./plugins/

    /// Max size of an uploaded .fpkg file
    pub max_package_size_bytes: u64,             // default: 50MB

    /// Default timeout for before-hooks
    pub hook_timeout_ms: u64,                    // default: 500

    /// Failures per window before circuit opens (per hook per plugin)
    pub circuit_open_threshold: u32,             // default: 10

    /// Window for counting failures
    pub circuit_window_secs: u64,                // default: 3600 (1 hour)

    /// How long before a tripped circuit is retried
    pub circuit_reset_secs: u64,                 // default: 300 (5 min)

    /// Max script plugin isolate memory
    pub script_max_memory_mb: u32,               // default: 64

    /// Max key-value cache per script plugin
    pub script_cache_quota_kb: u32,              // default: 512
}
```

### 10.2 Environment Variables

```bash
PLUGINS_DIR=/var/ferum/plugins           # where .fpkg files are extracted
PLUGINS_HOOK_TIMEOUT_MS=500              # global default hook timeout
PLUGINS_CIRCUIT_THRESHOLD=10            # failures before circuit opens
PLUGIN_INTERNAL_SECRET=<random-32-hex>  # HMAC key for Plugin Service Tokens
                                         # MUST be different from JWT_SECRET
```
