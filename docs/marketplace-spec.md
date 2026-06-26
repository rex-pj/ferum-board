# Ferum Board — Marketplace & Monetization Technical Specification

**Version:** 0.2-draft  
**Status:** Partially superseded — see notes below  
**Scope:** Plugin extension system, marketplace infrastructure, SaaS multi-tenancy layer

> **⚠ Compatibility note (2026-06-23):**
> Phase 1 (Plugin Extension System) has been implemented but with a different runtime than what this spec originally proposed.
>
> | Spec said | Actual implementation |
> |---|---|
> | WASM components + `wasmtime` | **boa_engine** — pure Rust JS engine, no native deps |
> | WIT interface definitions | `plugin.toml` manifest with `[[hooks]]` / `[ui_slots]` sections |
> | `ferum_manifest` custom WASM section | `plugin.toml` at root of `.fpkg` archive |
> | `PluginHost` port with `dispatch()` / `call_rpc()` | `PluginHookRuntime` + `PluginUiRuntime` + `PluginLifecycle` port traits |
> | Svelte `pluginSlots` store + `<PluginSlot>` component | Server-side injection into Tera context; client bundle.js served as static asset |
>
> Sections 2.1–2.9 below are historical reference only — **do not use as implementation guide**.
> For the actual plugin architecture see `docs/plugin-system/technical-design.md`.
>
> Phase 2 (Marketplace) and Phase 3 (SaaS) remain forward-looking and have not been started.

---

## Table of Contents

1. [Overview & Business Model](#1-overview--business-model)
2. [Phase 1 — Plugin Extension System](#2-phase-1--plugin-extension-system)
3. [Phase 2 — Marketplace Infrastructure](#3-phase-2--marketplace-infrastructure)
4. [Phase 3 — SaaS Multi-tenancy Layer](#4-phase-3--saas-multi-tenancy-layer)
5. [Database Schema Changes](#5-database-schema-changes)
6. [API Endpoints](#6-api-endpoints)
7. [Security Considerations](#7-security-considerations)
8. [Implementation Priority](#8-implementation-priority)

---

## 1. Overview & Business Model

Ferum Board adopts the **open-core** model: the core forum platform remains MIT-licensed and self-hosted, while revenue is generated through a surrounding commercial ecosystem — identical to NopCommerce's approach but with an additional SaaS layer NopCommerce lacks.

### Revenue Streams

| Stream | Mechanism | Target |
|---|---|---|
| **Marketplace commission** | 30% cut on every paid plugin/theme sale | Primary |
| **Official plugins** | First-party premium plugins (analytics, SSO, AI moderation) | Secondary |
| **SaaS hosting** | Managed Ferum Board on ferum.io (tiered subscriptions) | Growth |
| **Support packages** | Priority support SLAs for self-hosted deployments | Secondary |
| **Certification** | Verified developer program for marketplace contributors | Long-term |

### Non-Goals

- This spec does NOT cover changes to existing forum core functionality.
- Plugin system does NOT allow modifying core domain models at runtime.
- Multi-tenancy is additive — self-hosted single-tenant mode remains the default and is never broken.

---

## 2. Phase 1 — Plugin Extension System

### 2.1 Design Constraints (Rust-specific)

Rust does not support hot-loaded native shared libraries safely across compiler versions. The plugin system uses **WebAssembly (WASM) components** loaded by `wasmtime` as the sandboxed execution environment. This provides:

- Language independence (plugins can be written in Rust, Go, AssemblyScript, C, etc.)
- Safe sandboxing with explicit capability grants
- Hot-reload without restarting the host process
- A unique USP for marketing ("WASM-native plugin platform")

### 2.2 Plugin Lifecycle

```
plugin.wasm uploaded to admin panel
    ↓
PluginRegistry validates WASM component signature
    ↓
Metadata extracted (manifest.json embedded in WASM custom section)
    ↓
Admin enables plugin → PluginHost loads into wasmtime Engine
    ↓
EventBus routes ForumEvents to registered plugin hooks
    ↓
Plugin responses merged back into host process
```

### 2.3 Port Trait: `PluginHost`

Add to `backend/src/application/ports.rs`:

```rust
#[async_trait]
pub trait PluginHost: Send + Sync {
    /// Load and register a WASM plugin from storage.
    async fn load(&self, plugin_id: Uuid, wasm_key: &str) -> Result<(), AppError>;

    /// Unload a running plugin.
    async fn unload(&self, plugin_id: Uuid) -> Result<(), AppError>;

    /// Dispatch a forum event to all registered plugins.
    /// Returns collected SideEffects (additional jobs, notifications, etc.)
    async fn dispatch(&self, event: &ForumEvent) -> Result<Vec<PluginSideEffect>, AppError>;

    /// Call a named plugin RPC endpoint (for plugin-defined API routes).
    async fn call_rpc(
        &self,
        plugin_id: Uuid,
        method: &str,
        input: serde_json::Value,
        caller: Option<&AuthUser>,
    ) -> Result<serde_json::Value, AppError>;

    /// Return all loaded plugins and their status.
    fn list_loaded(&self) -> Vec<LoadedPluginInfo>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginSideEffect {
    EnqueueJob(ForumJob),
    CreateNotification { user_id: Uuid, kind: String, payload: serde_json::Value },
    AppendAuditLog { action: String, metadata: serde_json::Value },
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedPluginInfo {
    pub plugin_id: Uuid,
    pub slug: String,
    pub version: String,
    pub hooks: Vec<String>,
    pub status: PluginStatus,
}

#[derive(Debug, Clone, Serialize)]
pub enum PluginStatus {
    Running,
    Crashed { reason: String },
    Disabled,
}
```

### 2.4 Plugin Manifest Format

Each plugin WASM binary embeds a `ferum_manifest` custom section with JSON:

```json
{
  "schema_version": 1,
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "1.2.0",
  "min_core_version": "0.5.0",
  "author": "Developer Name",
  "description": "Short description under 200 chars.",
  "hooks": ["post_created", "thread_created", "user_banned"],
  "rpc_methods": ["get_stats", "reset_counters"],
  "required_capabilities": ["db_read", "cache_read_write", "http_outbound"],
  "settings_schema": {
    "type": "object",
    "properties": {
      "api_key": { "type": "string", "secret": true },
      "enabled": { "type": "boolean", "default": true }
    }
  }
}
```

**Capability model** — plugins explicitly request capabilities; host grants or denies at load time:

| Capability | What it grants |
|---|---|
| `db_read` | Read-only access to plugin_data table for this plugin's namespace |
| `db_write` | Read-write access to plugin_data table for this plugin's namespace |
| `cache_read_write` | Access to cache under `plugin:{slug}:*` key prefix |
| `http_outbound` | Make outbound HTTP requests (admin must whitelist domains) |
| `storage_read` | Read stored files via StorageService |
| `storage_write` | Upload new files via StorageService |

Plugins may NOT access core domain tables directly. All data exchange goes through the host-provided WASM imports.

### 2.5 Plugin WASM Interface (WIT Definition)

```wit
// ferum-plugin.wit
interface ferum-plugin {
  // Called by host on each registered event
  record post-created-event {
    post-id: string,
    thread-id: string,
    author-id: string,
    category-id: string,
  }
  on-post-created: func(event: post-created-event) -> list<side-effect>

  record thread-created-event {
    thread-id: string,
    author-id: string,
    category-id: string,
    title: string,
  }
  on-thread-created: func(event: thread-created-event) -> list<side-effect>

  // Named RPC, called by plugin API routes
  call-rpc: func(method: string, input: string) -> result<string, string>

  // Plugin metadata export — host reads this at load time
  get-manifest: func() -> string  // JSON string matching PluginManifest schema
}
```

### 2.6 Infrastructure Implementation

New file: `backend/src/infrastructure/plugin_host/wasm_plugin_host.rs`

```rust
pub struct WasmPluginHost {
    engine: wasmtime::Engine,
    loaded: RwLock<HashMap<Uuid, LoadedPlugin>>,
    storage: Arc<dyn StorageService>,
    cache: Arc<dyn CacheService>,
}

struct LoadedPlugin {
    meta: PluginMetadata,
    instance: wasmtime::component::Instance,
    store: Mutex<wasmtime::Store<PluginState>>,
}

struct PluginState {
    plugin_id: Uuid,
    db: DatabaseConnection,
    cache: Arc<dyn CacheService>,
    http_client: reqwest::Client,
    allowed_domains: Vec<String>,
}
```

Fallback for deployments without WASM support: `NullPluginHost` — implements `PluginHost` but returns empty results for all calls. Wired in `startup.rs` when `PLUGINS_ENABLED=false`.

### 2.7 EventBus Integration

Modify `backend/src/infrastructure/event_bus.rs` to dispatch to `PluginHost` after existing subscribers:

```rust
// After existing notification/webhook handling:
if let Some(plugin_host) = &self.plugin_host {
    let side_effects = plugin_host.dispatch(&event).await
        .unwrap_or_else(|e| {
            tracing::warn!("Plugin dispatch error: {}", e);
            vec![]
        });
    for effect in side_effects {
        self.apply_side_effect(effect).await;
    }
}
```

`plugin_host` field is `Option<Arc<dyn PluginHost>>` — absent when `PLUGINS_ENABLED=false`.

### 2.8 Plugin-defined Routes

Plugins can inject HTTP routes via their manifest's `rpc_methods`. The router assembly in `backend/src/routings/` adds a catch-all under `/api/plugins/{slug}/{method}`:

```
POST /api/plugins/:slug/:method
    → PluginRpcHandler
    → validate plugin is loaded and method exists
    → call plugin_host.call_rpc(plugin_id, method, body, auth_user)
    → return plugin's JSON response
```

Auth for plugin routes follows the same `Option<AuthUser>` injection as all other handlers. The plugin is responsible for its own authorization using the caller info passed in.

### 2.9 Frontend: Plugin UI Slots

Plugins may ship a `ui_bundle.js` (ESM) that exports Svelte 5 components registered to named slots.

Add to `client-app/src/lib/stores/plugins.ts`:

```ts
import { writable } from 'svelte/store';
import type { Component } from 'svelte';

export interface PluginSlotEntry {
  pluginSlug: string;
  component: Component;
  priority: number;  // lower = renders first
}

export const pluginSlots = writable<Record<string, PluginSlotEntry[]>>({});

export function registerPluginComponent(
  slotName: string,
  pluginSlug: string,
  component: Component,
  priority = 100,
) {
  pluginSlots.update(slots => {
    const list = slots[slotName] ?? [];
    return {
      ...slots,
      [slotName]: [...list, { pluginSlug, component, priority }]
        .sort((a, b) => a.priority - b.priority),
    };
  });
}
```

Add `client-app/src/lib/components/atoms/PluginSlot.svelte`:

```svelte
<script lang="ts">
  import { pluginSlots } from '$lib/stores/plugins';

  interface Props {
    name: string;
    context?: Record<string, unknown>;
  }

  let { name, context = {} }: Props = $props();
</script>

{#each $pluginSlots[name] ?? [] as entry (entry.pluginSlug)}
  <entry.component {context} />
{/each}
```

Named slots exposed in the forum UI:

| Slot Name | Location |
|---|---|
| `thread.header.after` | Below thread title, before first post |
| `thread.post.after` | Below each post |
| `thread.sidebar` | Thread page right sidebar |
| `category.header.after` | Below category description |
| `profile.tab` | User profile extra tab |
| `admin.sidebar.item` | Admin panel nav extension |
| `admin.dashboard.widget` | Admin dashboard extra card |

---

## 3. Phase 2 — Marketplace Infrastructure

### 3.1 Deployment Topology

The marketplace runs as a **separate application** at `marketplace.ferum.io`. It shares no code with the forum backend except the `PluginManifest` schema.

```
marketplace.ferum.io        ferum-board instance (self-hosted or ferum.io SaaS)
├── Plugin/theme listing    ├── Admin: "Install from marketplace"
├── Developer portal        ├── License validation (HTTP call to marketplace)
├── Stripe payment          ├── Auto-update checker
└── Review & ratings        └── Plugin registry (local DB)
```

The forum core communicates with the marketplace only for:
1. **License validation** at plugin load time.
2. **Update checks** (daily background job).
3. **Plugin binary download** when admin installs via URL.

The forum core never exposes its internal state to the marketplace.

### 3.2 License Validation Flow

```
Admin enables paid plugin
    ↓
PluginUseCase calls LicenseValidator.validate(slug, license_key, domain)
    ↓
LicenseValidator checks cache (TTL 24h) — returns immediately if hit
    ↓  (cache miss)
POST marketplace.ferum.io/api/v1/licenses/validate
    { slug, license_key, domain, core_version }
    ↓
Marketplace returns LicenseStatus
    ↓
LicenseValidator caches result (24h) and returns to use case
```

**LicenseStatus values:**

| Status | Meaning | Action |
|---|---|---|
| `Valid` | License active for this domain | Allow load |
| `InvalidKey` | Key does not exist | Block load, show error |
| `DomainMismatch` | Key registered to different domain | Block load, link to transfer instructions |
| `Expired` | Subscription lapsed | Block load, show renewal link |
| `GracePeriod { until }` | Subscription lapsed but within 14-day grace | Allow load, warn admin |

**Offline resilience:** If the marketplace API is unreachable, the validator falls back to the cached status. If no cache entry exists (first use offline), the plugin loads with a degraded-mode warning rather than failing hard — prevents disruption during marketplace outages.

### 3.3 Port Trait: `LicenseValidator`

Add to `backend/src/application/ports.rs`:

```rust
#[async_trait]
pub trait LicenseValidator: Send + Sync {
    async fn validate(
        &self,
        plugin_slug: &str,
        license_key: &str,
        domain: &str,
    ) -> Result<LicenseStatus, AppError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LicenseStatus {
    Valid,
    InvalidKey,
    DomainMismatch,
    Expired,
    GracePeriod { until: DateTime<Utc> },
    Free,         // Plugin is free, no validation needed
}
```

Implementations:
- `MarketplaceLicenseValidator` — HTTP call + cache in `infrastructure/license/`
- `NullLicenseValidator` — always returns `LicenseStatus::Free`. Used in self-hosted mode when `MARKETPLACE_URL` is unset.

### 3.4 Developer Revenue Split

```
Plugin sale $20 (configured by developer)
├── Developer receives: $14.00  (70%)
├── Platform fee:        $6.00  (30%)
│   ├── Stripe processing: ~$0.88
│   └── Net platform revenue: ~$5.12
```

Minimum payout threshold: $50 USD. Payouts via Stripe Connect (per-marketplace-developer account).

---

## 4. Phase 3 — SaaS Multi-tenancy Layer

### 4.1 Deployment Mode Enum

Add to `backend/src/config.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DeploymentMode {
    /// Default. Single forum, no tenant routing.
    SelfHosted,
    /// ferum.io managed hosting. Tenant resolved from hostname.
    SaaS,
}
```

`startup.rs` wires `TenantResolver` based on `DEPLOYMENT_MODE` env var.

### 4.2 Tenant Resolution

```rust
// application/ports.rs
#[async_trait]
pub trait TenantResolver: Send + Sync {
    async fn resolve(&self, host: &str) -> Result<TenantContext, AppError>;
}

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub slug: String,
    pub plan: TenantPlan,
    pub custom_domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TenantPlan {
    Community,   // free
    Starter,     // $19/mo
    Pro,         // $59/mo
    Enterprise,  // custom
}
```

Implementations:
- `SingleTenantResolver` — always returns a fixed `TenantContext` from env vars. Used in `SelfHosted` mode.
- `PgTenantResolver` — looks up tenant by hostname in `tenants` table. Used in `SaaS` mode.

### 4.3 Row Level Security (PostgreSQL)

In SaaS mode, every core table gains a `tenant_id UUID NOT NULL` column. RLS policies enforce tenant isolation at the database level — a second line of defense after application-layer filtering:

```sql
ALTER TABLE threads ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON threads
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
```

The application sets `app.tenant_id` per connection via `SET LOCAL app.tenant_id = '...'` inside a transaction. This is handled in a middleware layer that runs before every DB query in SaaS mode.

In `SelfHosted` mode, `tenant_id` columns exist but RLS policies are NOT created — zero runtime overhead for self-hosted deployments.

### 4.4 SaaS Tier Limits

| Limit | Community | Starter | Pro | Enterprise |
|---|---|---|---|---|
| Max users | 100 | 1,000 | Unlimited | Custom |
| Max categories | 5 | 20 | Unlimited | Custom |
| Custom domain | No | No | Yes | Yes |
| Plugins | 0 | 3 | Unlimited | Custom |
| Storage | 500 MB | 5 GB | 50 GB | Custom |
| API rate limits | Standard | Standard | Elevated | Custom |
| SSO (SAML/OIDC) | No | No | No | Yes |
| SLA | None | None | 99.9% | 99.99% |

Limit enforcement lives in a `PlanEnforcer` struct called by use cases before operations that consume quota:

```rust
// application/plan_enforcer.rs
pub struct PlanEnforcer {
    limits: TenantPlanLimits,
}

impl PlanEnforcer {
    pub fn check_user_limit(&self, current_count: u64) -> Result<(), AppError>;
    pub fn check_category_limit(&self, current_count: u64) -> Result<(), AppError>;
    pub fn check_plugin_limit(&self, current_count: u64) -> Result<(), AppError>;
    pub fn check_storage_bytes(&self, current_bytes: u64, new_bytes: u64) -> Result<(), AppError>;
}
```

### 4.5 SaaS Tenant Onboarding Flow

```
POST ferum.io/signup
    ↓
Create tenant row (plan=Community)
Create subdomain [slug].ferum.io
Provision isolated Postgres schema OR add tenant_id seed rows
Send confirmation email
    ↓
Tenant admin completes setup wizard (same /setup route as self-hosted)
    ↓
Tenant active
```

For Pro and above: custom domain verified via DNS CNAME check. `PgTenantResolver` checks `custom_domain` column and maps hostname → tenant.

---

## 5. Database Schema Changes

### 5.1 Plugin Tables (Phase 1)

```sql
-- Plugin registry
CREATE TABLE plugins (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug           VARCHAR(100) UNIQUE NOT NULL,
    name           VARCHAR(200) NOT NULL,
    version        VARCHAR(50) NOT NULL,
    wasm_storage_key VARCHAR(500) NOT NULL,  -- key in StorageService
    manifest       JSONB NOT NULL,
    is_enabled     BOOLEAN NOT NULL DEFAULT false,
    config         JSONB NOT NULL DEFAULT '{}',
    license_key    VARCHAR(500),
    license_status VARCHAR(50),              -- cached from last validation
    license_checked_at TIMESTAMPTZ,
    installed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    installed_by   UUID REFERENCES users(id)
);

-- Isolated key-value storage per plugin (plugins may not touch core tables)
CREATE TABLE plugin_data (
    plugin_slug    VARCHAR(100) NOT NULL,
    key            VARCHAR(500) NOT NULL,
    value          JSONB,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (plugin_slug, key)
);

-- Indexes
CREATE INDEX idx_plugin_data_slug ON plugin_data (plugin_slug);
```

### 5.2 Tenant Tables (Phase 3 — SaaS only migration)

```sql
-- Created only when DEPLOYMENT_MODE=saas
CREATE TABLE tenants (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug           VARCHAR(100) UNIQUE NOT NULL,
    display_name   VARCHAR(200) NOT NULL,
    plan           VARCHAR(50) NOT NULL DEFAULT 'community',
    custom_domain  VARCHAR(500) UNIQUE,
    is_active      BOOLEAN NOT NULL DEFAULT true,
    stripe_customer_id VARCHAR(200),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- tenant_id column added to all core tables via migration:
-- users, categories, threads, posts, reactions, notifications,
-- reports, audit_logs, bookmarks, webhooks, plugins
ALTER TABLE users ADD COLUMN tenant_id UUID REFERENCES tenants(id);
-- ... (repeated for each table)

-- Partial index: tenant_id is required only in SaaS mode.
-- In self-hosted mode the column exists but is always NULL.
CREATE INDEX idx_users_tenant ON users (tenant_id) WHERE tenant_id IS NOT NULL;
```

### 5.3 Migration Strategy

Migrations are versioned in `backend/migration/src/`. New files follow the existing `m{timestamp}_{description}.rs` naming:

| File | Content |
|---|---|
| `m20260601_000001_create_plugins.rs` | Plugin tables (Phase 1) |
| `m20260601_000002_create_plugin_slots.rs` | Frontend slot registry table |
| `m20260901_000001_create_tenants.rs` | Tenant table (Phase 3, guarded by env var) |
| `m20260901_000002_add_tenant_id_columns.rs` | tenant_id on all core tables (Phase 3) |

Phase 3 migrations are conditional: they check `DEPLOYMENT_MODE` env var and skip if `self_hosted`. This keeps self-hosted schema clean.

---

## 6. API Endpoints

### 6.1 Plugin Management (Admin only)

All routes under `/api/admin/plugins` require `RequireAdmin`.

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/admin/plugins` | List all installed plugins |
| `POST` | `/api/admin/plugins` | Install plugin (upload WASM or provide marketplace URL) |
| `GET` | `/api/admin/plugins/:id` | Plugin detail + settings schema |
| `PATCH` | `/api/admin/plugins/:id` | Update config or enable/disable |
| `DELETE` | `/api/admin/plugins/:id` | Uninstall plugin |
| `POST` | `/api/admin/plugins/:id/reload` | Hot-reload WASM (without config reset) |
| `POST` | `/api/admin/plugins/:id/validate-license` | Force re-check license with marketplace |

### 6.2 Plugin RPC (Auth level defined per plugin)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/plugins/:slug/:method` | Call plugin-defined RPC method |
| `GET` | `/api/plugins/:slug/assets/*path` | Serve plugin static assets |

### 6.3 SaaS Tenant Management (Phase 3)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/saas/tenants` | Create new tenant (signup) |
| `GET` | `/api/saas/tenants/me` | Current tenant info + plan |
| `PATCH` | `/api/saas/tenants/me` | Update display name, custom domain |
| `POST` | `/api/saas/tenants/me/verify-domain` | Trigger DNS CNAME check |
| `GET` | `/api/saas/tenants/me/usage` | Current quota usage |
| `GET` | `/api/saas/billing/portal` | Redirect to Stripe billing portal |

---

## 7. Security Considerations

### 7.1 WASM Plugin Sandbox

- Each plugin runs in an isolated `wasmtime::Store` with no shared memory.
- Outbound HTTP is routed through a host-provided client that enforces domain allowlists.
- Plugins cannot access the filesystem, environment variables, or OS syscalls.
- CPU time per `dispatch()` call is capped (wasmtime fuel metering): default 10M instructions.
- Memory per plugin instance is capped: default 64 MB.
- Plugin crashes are caught and do not panic the host process.

### 7.2 Plugin Installation

- WASM binary is scanned for prohibited import namespaces before loading.
- Manifest signature verification: marketplace-distributed plugins are signed with an Ed25519 key; self-installed plugins may bypass signing (admin opt-in).
- Admin audit log records every install, uninstall, enable, disable, config change.

### 7.3 License Key Storage

- License keys are stored encrypted at rest using the application's `JWT_SECRET` as the key derivation input.
- Keys are never returned in API responses — only their validation status is exposed.
- The `/api/admin/plugins/:id` response shape omits `license_key` and returns only `license_status` and `license_checked_at`.

### 7.4 Multi-tenant Data Isolation

- `TenantContext` is resolved in middleware and injected into every use case call.
- Use cases pass `tenant_id` to repository methods; repositories include it in all WHERE clauses.
- RLS at the DB level provides a second enforcement layer.
- Cross-tenant API calls return 404 (not 403) to avoid tenant enumeration.

---

## 8. Implementation Priority

### Phase 1 Deliverables (Plugin System)

**Prerequisites:** No existing features are broken. Plugin system is entirely additive.

| Step | File(s) | Complexity |
|---|---|---|
| 1. Add `PluginHost` port trait | `application/ports.rs` | Low |
| 2. Add `ForumJob::RunPluginHook` variant | `application/ports.rs` | Low |
| 3. Create plugin DB migration | `migration/src/` | Low |
| 4. Create `PluginData` domain model + repo trait | `domain/models/`, `domain/repositories/` | Low |
| 5. Create `PluginUseCase` | `application/usecases/plugin_usecase.rs` | Medium |
| 6. Create `WasmPluginHost` | `infrastructure/plugin_host/` | High |
| 7. Create `NullPluginHost` fallback | `infrastructure/plugin_host/` | Low |
| 8. Wire into `EventBus` | `infrastructure/event_bus.rs` | Low |
| 9. Wire into `startup.rs` | `startup.rs` | Low |
| 10. Add plugin admin handlers + routes | `handlers/`, `routings/` | Medium |
| 11. Add plugin RPC catch-all route | `routings/` | Medium |
| 12. Add `PluginSlot.svelte` atom | `client-app/src/lib/components/atoms/` | Low |
| 13. Add plugin slot store | `client-app/src/lib/stores/plugins.ts` | Low |
| 14. Add slots to key page organisms | `client-app/src/lib/components/organisms/` | Medium |
| 15. Add plugin admin pages | `client-app/src/routes/(admin)/admin/plugins/` | Medium |

**Cargo dependencies to add:**

```toml
# backend/Cargo.toml
wasmtime = { version = "25", features = ["component-model"] }
wasmtime-wasi = "25"
```

### Phase 2 Deliverables (Marketplace)

Built as a **separate SvelteKit + Rust application**. Does not modify ferum-board core except:

| Step | File(s) | Complexity |
|---|---|---|
| 1. Add `LicenseValidator` port trait | `application/ports.rs` | Low |
| 2. Add `license_key`, `license_status` to Plugin model | domain + migration | Low |
| 3. Create `MarketplaceLicenseValidator` | `infrastructure/license/` | Medium |
| 4. Create `NullLicenseValidator` | `infrastructure/license/` | Low |
| 5. Call validator in `PluginUseCase::enable` | `application/usecases/plugin_usecase.rs` | Low |
| 6. Wire in `startup.rs` (check `MARKETPLACE_URL` env var) | `startup.rs` | Low |
| 7. Add license status to plugin admin UI | `client-app/src/routes/(admin)/admin/plugins/` | Medium |

### Phase 3 Deliverables (SaaS)

**Requires:** Phase 1 complete. Highest risk — DB schema changes affect every table.

| Step | File(s) | Complexity |
|---|---|---|
| 1. Add `DeploymentMode` enum to config | `config.rs` | Low |
| 2. Add `TenantResolver` port trait | `application/ports.rs` | Low |
| 3. Create `tenants` table migration | `migration/src/` | Low |
| 4. Create `tenant_id` column migrations (conditional) | `migration/src/` | Medium |
| 5. Create `PgTenantResolver` + `SingleTenantResolver` | `infrastructure/` | Medium |
| 6. Add `TenantContext` extractor middleware | `middleware/` | Medium |
| 7. Update all repository methods to accept `tenant_id` | `infrastructure/repositories/` | High |
| 8. Create `PlanEnforcer` | `application/plan_enforcer.rs` | Medium |
| 9. Call `PlanEnforcer` in relevant use cases | `application/usecases/` | Medium |
| 10. SaaS tenant API handlers | `handlers/`, `routings/` | Medium |
| 11. Stripe billing integration | `infrastructure/billing/` | High |
| 12. SaaS signup + onboarding frontend | New SvelteKit app or ferum.io pages | High |

### Suggested Quarterly Roadmap

| Quarter | Goal |
|---|---|
| **Q3 2026** | Phase 1 complete: WASM plugin host working. 3–5 official first-party plugins (analytics widget, custom themes, spam filter) available as seed content. |
| **Q4 2026** | Phase 2: Marketplace MVP live. Developer portal open. Revenue sharing active. |
| **Q1 2027** | Phase 2 maturity: review system, featured listings, developer certification program. |
| **Q2 2027** | Phase 3: SaaS beta on ferum.io. Community and Starter tiers live. |
| **Q3 2027** | Phase 3 maturity: Pro tier, custom domains, enterprise pipeline. |
