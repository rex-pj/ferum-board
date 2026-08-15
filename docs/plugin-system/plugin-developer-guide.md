# Plugin Developer Guide

How to write, package and install a Ferum Board plugin.

Everything here is drawn from working code. The seven plugins under
`examples/plugins/` are the reference implementations — they are packaged by
`scripts/package-examples.*` and their custom-element names are asserted against the
server's naming rule by `backend/tests/domain/src/models/plugin.rs`, so they cannot
drift out of sync with the runtime.

For how the runtime works internally, see [technical-design.md](technical-design.md).

## Contents

1. [Tiers](#1-tiers)
2. [Package format](#2-package-format)
3. [Tier 1 — Manifest plugin](#3-tier-1--manifest-plugin)
4. [Tier 2 — Script plugin](#4-tier-2--script-plugin)
5. [UI slots](#5-ui-slots)
6. [RPC actions](#6-rpc-actions)
7. [Plugin-owned SQL](#7-plugin-owned-sql)
8. [Installing and managing plugins](#8-installing-and-managing-plugins)
9. [Reference: plugin.toml](#9-reference-plugintoml)
10. [Reference: the Ferum JavaScript API](#10-reference-the-ferum-javascript-api)
11. [Reference: hooks and events](#11-reference-hooks-and-events)
12. [Troubleshooting](#12-troubleshooting)

---

## 1. Tiers

`meta.tier` in the manifest selects the runtime.

| Tier | Value | What it can do | Status |
| --- | --- | --- | --- |
| 1 | `manifest` | Declare outbound webhooks. No code runs on the server. | Implemented |
| 2 | `script` | Run JavaScript server-side in a sandbox: before-hooks, after-events, RPC actions, plugin-owned SQL, UI slots. | Implemented, behind the `script_plugins` cargo feature (on by default) |
| 3 | `service` | Sidecar process. | **Not implemented.** The manifest parses and the plugin installs, but every hook resolves to "allow" without running anything (`registry.rs`). Do not build against it. |

Choose Tier 1 if all you need is "when X happens, POST somewhere". Choose Tier 2 for
anything else. Every non-trivial example in this repo — a chatbox, a poll widget, a
homepage masthead with its own image uploads — is Tier 2.

**Tier 2 requires the host to be built with `script_plugins`.** It is a default
feature, but a deployment built with `--no-default-features` has no JavaScript engine
and will report Script plugins as unsupported.

## 2. Package format

A plugin is a ZIP archive with the extension `.fpkg`. There is no packaging CLI; the
repo's own packager is a shell script.

```
my-plugin.fpkg
├── plugin.toml     required — the manifest
├── bundle.js       Tier 2: one file, evaluated by the sandbox and also served
│                   to the browser when the plugin declares a UI slot
└── README.md       optional
```

That is the whole format. `bundle.js` is a single pre-bundled file — the sandbox does
not resolve imports, so whatever build tool you prefer must emit one file.

To package by hand:

```bash
cd my-plugin
zip -X -r ../my-plugin-1.0.0.fpkg . -x '.*'
```

To repackage the bundled examples after editing one of their source directories:

```powershell
pwsh scripts/package-examples.ps1          # Windows
pwsh scripts/package-examples.ps1 -Check   # verify archives match sources (per-file SHA-256)
```

```bash
scripts/package-examples.sh                # Linux; needs zip + unzip
scripts/package-examples.sh --check
```

Never hand-edit an `.fpkg` in `examples/` — they are build artifacts, and `-Check`
compares content hashes, not just file names.

## 3. Tier 1 — Manifest plugin

A Tier 1 plugin declares webhook subscriptions and nothing else. Ferum registers the
URL and posts to it through the existing webhook fan-out when the event fires.

Complete working example: `examples/plugins/discord-notifier/`.

```toml
[meta]
id          = "com.example.discord-notifier"
name        = "Discord Notifier"
version     = "1.0.0"
tier        = "manifest"
min_ferum   = "0.1.0"
description = "Posts to a Discord channel webhook when a thread or post is created."
author      = "Your Name"

# Rendered as a read-only field reference under the config editor in the admin
# panel. These descriptions are the only documentation an operator sees.
[config_schema]
type     = "object"
required = ["webhook_url"]

[config_schema.properties.webhook_url]
type        = "string"
title       = "Discord Webhook URL"
description = "Settings → Integrations → Webhooks in your Discord server."

[config_schema.properties.username]
type        = "string"
title       = "Bot display name"
default     = "Ferum Board"

# {{config.field}} is substituted from the saved config at activation time.
# This substitution applies to webhook URLs only — see the note on ui_slots props.
[[webhooks]]
event = "post.created"
url   = "{{config.webhook_url}}"

[capabilities]
hooks = []
```

The payload is Ferum's standard webhook body, signed with
`X-Ferum-Signature: sha256=<hmac>`. It is not Discord-specific; if you need a
particular body shape, use Tier 2 and build the request yourself with
`Ferum.http.post`.

## 4. Tier 2 — Script plugin

`bundle.js` is evaluated once per plugin in a `boa_engine` context. It registers
handlers by assigning into two globals the host provides:

```js
__ferum_hooks['before_post_create'] = function (ctx) { /* … */ };
__ferum_rpc['send_message']         = function (ctx) { /* … */ };
```

Three things to know before writing any of it:

- **The sandbox is synchronous.** There is no event loop, no `Promise`, no
  `async`/`await`, no `setTimeout`. `Ferum.http.get` blocks and returns the parsed
  body. Write plain synchronous code.
- **There is no module system and no DOM.** No `import`, no `require`, no `fetch`, no
  `window`. `boa_engine` implements ECMAScript, not a browser.
- **A hook has a deadline.** `PLUGIN_HOOK_TIMEOUT_MS` (default 500 ms) applies to each
  `before_*` call. Exceeding it kills the call and the request proceeds as if you had
  allowed it.

### A before-hook

Complete working example: `examples/plugins/profanity-filter/`.

The handler receives `ctx` and returns a decision object:

```js
(function () {
    'use strict';

    function checkContent(ctx) {
        // ctx.hook_name          — e.g. "before_post_create"
        // ctx.actor_id           — UUID string, or null for a guest
        // ctx.actor_trust_level  — "new" | "basic" | "member" | "regular" | "leader"
        // ctx.payload            — hook-specific; see section 11
        var payload = ctx.payload || {};
        var text = (payload.title || '') + ' ' + (payload.content_md || '');

        if (isBad(text)) {
            Ferum.log.warn('Blocked', { hook: ctx.hook_name, actor_id: ctx.actor_id });
            return {
                deny: {
                    reason: Ferum.config.block_message,  // shown to the user
                    error_code: 'profanity_blocked',     // machine code, 403
                },
            };
        }
        return { allow: true };
    }

    __ferum_hooks['before_post_create']   = checkContent;
    __ferum_hooks['before_post_edit']     = checkContent;
    __ferum_hooks['before_thread_create'] = checkContent;
})();
```

Return `{ allow: true }` or `{ deny: { reason, error_code } }`. Anything else, a
thrown exception, or a timeout is treated as **allow** — before-hooks fail open, so a
broken plugin degrades the forum rather than breaking it. Do not rely on a hook as
your only line of defence for anything security-critical.

The manifest must declare each hook, with a priority (lower runs first):

```toml
[capabilities]
hooks = [
    { name = "before_post_create",   priority = 10 },
    { name = "before_post_edit",     priority = 10 },
    { name = "before_thread_create", priority = 10 },
]
```

A hook the manifest does not declare is never dispatched, even if `bundle.js`
registers it. The runtime intersects what the manifest asks for with what the admin
granted at install time.

## 5. UI slots

A UI slot injects a custom element into a named position in the page.

```toml
[ui_slots.home_feed_top]
component = "/plugins/com.example.my-plugin/assets/bundle.js"
props     = ['data-message="Hello"', 'data-kind="info"']
```

At activation the host inserts one row per slot into `plugin_ui_slots`. On every page
request it emits, in `load_order` then slug order:

```html
<ferum-slot-com-example-my-plugin-home-feed-top data-message="Hello" data-kind="info">
</ferum-slot-com-example-my-plugin-home-feed-top>
```

**Your `bundle.js` must define exactly that element name.** It is
`ferum-slot-` + the plugin id + the slot name, each lowercased with every run of
non-alphanumeric characters folded to a single `-`. The name is derived by the server
on every read, so it is not configurable and not negotiable. An element nobody defines
renders as an empty inline box with no error anywhere.

The slug is part of the name so that two plugins can occupy the same slot. Named after
the slot alone they would both call `customElements.define` with the same string; the
first would render into both elements and the second into neither.

Slots rendered by the built-in theme:

| Slot | Rendered in | Appears on |
| --- | --- | --- |
| `content_before` | `base.html` | every page |
| `content_after` | `base.html` | every page |
| `navbar_end` | `partials/nav.html` | every page |
| `sidebar_left_top` | `partials/sidebar_left.html` | pages with the left sidebar |
| `home_feed_top` | `home.html` | the homepage only |

A slot name is just a string — the host will happily store any name, but nothing
renders unless a template asks for `plugin_slots.<name>`. A custom theme that
overrides `home.html` must render `plugin_slots.home_feed_top` itself; `base.html`
only carries `content_before` and `content_after`. All three example themes override
`home.html` and do render it.

The same `bundle.js` file is evaluated server-side *and* served to the browser, so
guard each half:

```js
if (typeof __ferum_rpc === 'object') {
    // server-side: Ferum.* exists, DOM does not
}
if (typeof customElements === 'object') {
    // browser: fetch and DOM exist, Ferum.* does not
}
```

`examples/plugins/simple-chatbox/bundle.js` is the reference for this pattern.

**`props` are baked in at activation and are not re-read from config.** Editing the
plugin config does not change them until the plugin is deactivated and activated
again. If your widget's content is meant to be editable, declare no props and have
the element fetch its content through an RPC action instead —
`examples/plugins/home-hero/` does exactly this, and config edits there take effect on
the next page load.

An admin can move a slot without reinstalling:
`PATCH /api/admin/plugins/:slug/ui-slots/:slot_id`.

## 6. RPC actions

An RPC action is a plugin-defined endpoint:

```
POST /api/plugins/{plugin_id}/rpc/{action}
```

Declare the action names, or the call is rejected:

```toml
[capabilities]
rpc = ["send_message", "get_history"]
```

Register the handler:

```js
__ferum_rpc['send_message'] = function (ctx) {
    if (!ctx.actor_id) {
        return { ok: false, error: 'Please log in to send a message.' };
    }
    var text = String((ctx.payload.body || {}).text || '').trim();
    if (!text) { return { ok: false, error: 'Message text is required' }; }

    var history = Ferum.storage.get('messages') || [];
    history.push({ username: ctx.payload.actor_username, text: text, at: Ferum.utils.now() });
    Ferum.storage.set('messages', history);

    return { ok: true, data: history[history.length - 1] };
};
```

For an RPC call, `ctx` is:

| Field | Value |
| --- | --- |
| `ctx.hook_name` | `"rpc:<action>"` |
| `ctx.actor_id` | caller's UUID, or `null` |
| `ctx.actor_trust_level` | trust level, or `"guest"` |
| `ctx.payload.body` | the JSON request body, or `null` |
| `ctx.payload.actor_username` | caller's username, or `null` |
| `ctx.payload.actor_display_name` | caller's display name, or `null` |

**This endpoint does not require authentication, by design.** Guests are 70% of the
audience and most plugins split into public reads and member-only writes; the host
cannot tell which is which for an arbitrary action. **Every write action must check
`ctx.actor_id` itself**, as above. The endpoint is rate-limited as a public write.

Unlike before-hooks, RPC does **not** fail open: an unknown or ungranted action is a
real 4xx to the caller.

There is also a multipart image upload for plugins that need one:

```
POST /api/plugins/{plugin_id}/media
```

It requires `granted_capabilities.media` and stores into a CAS namespace of
`plugin_{slug}`, so a plugin can never collide with core files or another plugin's. It
is separate from RPC because base64-ing a multi-megabyte file through the
single-threaded sandbox would blow the hook timeout.

## 7. Plugin-owned SQL

A plugin can own a real relational schema. Declare it:

```toml
[capabilities]
db = true

[schema]
tables = [
    "CREATE TABLE polls (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), question TEXT NOT NULL)",
    "CREATE TABLE poll_votes (poll_id UUID NOT NULL REFERENCES polls(id) ON DELETE CASCADE, user_id UUID NOT NULL, PRIMARY KEY (poll_id, user_id))",
]
```

The statements run once at install, into a dedicated PostgreSQL schema named
`plugin_{slug}`. Query it with:

```js
var rows = Ferum.db.query(
    'SELECT row_to_json(p) FROM polls p WHERE id = $1',
    [pollId]
);
```

Rules that come from the runtime, not from convention:

- **Parameters are positional and always bound.** Never interpolate a value into the
  SQL string.
- **A `SELECT` or writable `WITH` must return a single JSON/JSONB column.** Wrap rows
  with `row_to_json(...)` or `jsonb_agg(...)`. Other statements return
  `{ rows_affected }`.
- **You cannot read core tables.** The query runs under
  `SET LOCAL ROLE ferum_plugin` with `search_path` set to your schema. That role holds
  no grant on any application table, so `SELECT … FROM public.users` is refused by
  PostgreSQL itself, not by string matching. A second layer (`validate_plugin_sql`)
  independently rejects DDL and multi-statement input.
- **Plugin SQL runs under a 2-second statement timeout.**

Complete working example: `examples/plugins/community-polls/` — Tier 2, with its own
three-table schema, RPC actions and a UI slot. Note that this contradicts anything you
may read elsewhere claiming `[db]` is a Tier 3 feature; it is not.

## 8. Installing and managing plugins

### Install

1. **Admin → Plugins → Upload**. Select the `.fpkg`.
2. Review the capability summary the server extracts from the manifest — hooks, RPC
   actions, `db`, `media`, webhook targets — then confirm. What you grant here is
   intersected with what the manifest declares at every dispatch; a capability you do
   not grant is unreachable even if the code tries to use it.
3. **Activate** on the plugin list. Activation is what writes the `plugin_ui_slots`
   rows and registers hooks.

Requirements: the `admin.plugins` permission, and for Tier 2 a host built with the
`script_plugins` feature. The package ceiling is 50 MB.

### Manage

| Action | Where |
| --- | --- |
| Activate / deactivate | `PATCH /api/admin/plugins/:slug/status`, or the buttons on the list page |
| Edit config | `PATCH /api/admin/plugins/:slug/config`, or the detail page |
| Read execution logs | `GET /api/admin/plugins/:slug/logs`, or the detail page |
| Move a UI slot | `PATCH /api/admin/plugins/:slug/ui-slots/:slot_id` — takes effect immediately |
| Uninstall | `DELETE /api/admin/plugins/:slug` |

The config editor is a **raw JSON textarea**, not a generated form. `[config_schema]`
is rendered beneath it as a read-only field reference: title, type, default, required,
description. That reference is the only place your plugin's documentation reaches the
operator, so put any constraint the server cannot enforce into a `description`. A rule
that lives only in your README reaches nobody.

Installed plugins are extracted to `PLUGINS_DIR/{slug}/`. Their assets are served at
`/plugins/{slug}/assets/{path}` — and only that subtree, so manifests and hook sources
are not reachable by anonymous callers.

### Development loop

There is no hot reload for plugin code. After editing `bundle.js`: repackage, upload,
and reinstall. Config changes take effect without reinstalling; `props` changes do not
(see [section 5](#5-ui-slots)).

## 9. Reference: `plugin.toml`

### `[meta]` — required, all tiers

| Field | Required | Notes |
| --- | --- | --- |
| `id` | yes | Reverse-domain, e.g. `com.example.my-plugin`. Alphanumerics, `.`, `-`, `_` only; may not start or end with `.`; max 256 chars. This is the slug used in URLs and in the custom-element name. |
| `name` | yes | Display name. |
| `version` | yes | |
| `tier` | yes | `manifest`, `script` or `service`. Anything else is rejected at parse time. |
| `min_ferum` | no | |
| `description` | no | Shown in the admin list. |
| `author` | no | |

### `[capabilities]`

| Field | Type | Meaning |
| --- | --- | --- |
| `hooks` | array of `{ name, priority }` | Before-hooks and after-events to subscribe to. Lower priority runs first. |
| `rpc` | array of strings | Action names callable at `/api/plugins/:slug/rpc/:action`. |
| `api` | array of strings | `Ferum.forum.*` functions this plugin may call, e.g. `"forum.createNotification"`. |
| `db` | boolean | Enables `Ferum.db.query` and the `[schema]` section. |
| `http_allowlist` | array of host names | Hosts `Ferum.http.*` may call. **An empty or absent list disables outbound HTTP entirely** — there is no separate on/off flag. A host matches exactly or as a subdomain: `example.com` also permits `api.example.com`. |
| `media` | boolean | Enables `POST /api/plugins/:slug/media`. |

Every one of these is granted by the admin at install and intersected with the
manifest at dispatch time. A capability present in the manifest but not granted is
unreachable, and so is a capability granted but not declared — the runtime takes the
intersection precisely so a plugin author cannot ship a wider allowlist than the one
that was reviewed.

### `[script]` — Tier 2

| Field | Meaning |
| --- | --- |
| `bundle_file` | The single JavaScript file to evaluate. Every example uses `bundle.js`. |

### `[config_schema]`

A JSON-Schema-shaped object: `type`, `required`, and `properties.<name>` entries with
`type`, `title`, `description`, `default` and `secret`. `default` values are seeded into
the plugin's config at install, so a plugin can be useful the moment it is activated.

#### `secret = true` — mark every credential

```toml
[config_schema.properties.webhook_url]
type   = "string"
title  = "Discord Webhook URL"
secret = true
```

A field carrying `secret = true` is **encrypted at rest** in `plugins.config`
(XChaCha20-Poly1305) whenever the operator has set `SECRET_ENCRYPTION_KEY`. Without the
flag the value sits in plaintext in the table, and therefore in every database backup.

Set it on anything that grants access on possession: passwords, API keys, bearer tokens,
and **capability URLs** — a Discord webhook URL needs no further authentication, so
holding it is holding the credential. When unsure, set it; the cost is nil, and the
value still round-trips through the admin config editor in the clear.

Two consequences worth knowing:

- The ciphertext is bound to *this plugin's slug and this field name*, so a value copied
  between plugins or between fields fails to decrypt rather than silently working.
- Adding the flag to a plugin that is already installed is safe. Existing plaintext reads
  back unchanged and is sealed on the next config save or the next restart.

Ferum cannot infer which fields are credentials — only the manifest knows — so an
unflagged secret is stored exactly as written.

### `[ui_slots.<slot_name>]`

| Field | Meaning |
| --- | --- |
| `component` | Path at which the browser fetches the bundle, relative to `APP_URL`. |
| `props` | Static HTML attribute strings added to the element at activation. Not re-resolved from config. |

### `[[webhooks]]` — Tier 1

| Field | Meaning |
| --- | --- |
| `event` | Event name, e.g. `post.created`. |
| `url` | Target URL. `{{config.field}}` is substituted from the saved config at activation. |

### `[schema]` — requires `capabilities.db = true`

| Field | Meaning |
| --- | --- |
| `tables` | Array of `CREATE TABLE` statements, run once at install into `plugin_{slug}`. |

## 10. Reference: the Ferum JavaScript API

Available to server-side Tier 2 code. All calls are synchronous.

```js
Ferum.config                          // the saved config object (read-only)

Ferum.log.info(msg, ctx?)             // → plugin_logs, visible in the admin UI
Ferum.log.warn(msg, ctx?)
Ferum.log.error(msg, ctx?)
Ferum.log.trace(msg, ctx?)

// TTL cache. Plugin-namespaced; string values only; may be evicted.
Ferum.cache.get(key)                  // string | null
Ferum.cache.set(key, value, ttlSecs)
Ferum.cache.del(key)

// Durable KV (plugin_storage table). Values are arbitrary JSON.
// Persists until deleted or the plugin is uninstalled.
Ferum.storage.get(key)                // any | null
Ferum.storage.set(key, value)
Ferum.storage.del(key)
Ferum.storage.list(prefix, limit)     // default limit 100

// Requires a non-empty capabilities.http_allowlist. 10-second timeout.
Ferum.http.get(url, headers?)         // parsed JSON | null
Ferum.http.post(url, body, headers?)  // body is serialised for you

Ferum.utils.sha256(input)             // hex string
Ferum.utils.now()                     // ISO 8601
Ferum.utils.slugify(input)

// Capability-gated: each name must be in capabilities.api AND granted.
Ferum.forum.getUserPublic(userId)     // object | null
Ferum.forum.createNotification(userId, message)

// Requires capabilities.db = true. See section 7.
Ferum.db.query(sql, params)
```

`Ferum.storage` and `Ferum.cache` are namespaced by plugin at the repository layer. A
plugin cannot read another plugin's keys regardless of what key string it passes.

**Neither is encrypted at rest, and neither may hold a credential.** Namespacing keeps
other *plugins* out; it does nothing about a database dump, a backup, or anyone with
read access to `plugin_storage`. There is no `secret = true` equivalent here, because
the values are arbitrary JSON written at runtime and nothing declares their shape in
advance. A token a plugin needs to keep must live in a `secret = true` **config** field,
which the operator sets and Ferum encrypts. The same applies to `Ferum.log.*` — the
`ctx` object is stored verbatim in `plugin_logs` and shown in the admin UI, so never log
a credential, a session token, or a raw request body that might carry one.

Outbound HTTP is guarded twice: the host must be in the granted `http_allowlist`, and
the request then goes through a client pinned to the resolved address, so a name that
resolves to a private range — or re-resolves to one between the check and the connect
— is refused.

## 11. Reference: hooks and events

### Before-hooks

Dispatched synchronously; can deny the request. Fail open.

| Hook | `ctx.payload` includes |
| --- | --- |
| `before_user_register` | registration fields |
| `before_post_create` | `content_md`, `thread_id` |
| `before_post_edit` | `content_md` |
| `before_post_delete` | post identity |
| `before_thread_create` | `title`, `content_md`, `category_id` |
| `before_thread_delete` | thread identity |
| `before_reaction_add` | post and reaction kind |
| `before_best_answer_mark` | post and thread identity |
| `before_user_ban` | target user and reason |

Payload shapes are built at the call site in the corresponding use case
(`ferum-application/src/usecases/`); read that file when you need the exact fields for
a hook. Log `ctx.payload` from a `Ferum.log.info` call during development to see
precisely what arrives.

### After-events

Fire-and-forget; the return value is ignored and nothing can be blocked. Subscribe by
listing the event name in `capabilities.hooks`. The event names are the
`event_type_str()` values of `ForumEvent` (`ferum-domain/src/events.rs`):

`post.created`, `post.deleted`, `thread.created`, `thread.deleted`, `thread.locked`,
`thread.moved`, `user.banned`, `user.warned`, `user.trust_level_changed`,
`reaction.added`, `reaction.removed`, `best_answer.marked`, `mention.added`,
`user.followed`.

The same names are what a Tier 1 `[[webhooks]]` entry subscribes to.

### Circuit breaker

Consecutive failures — timeouts or thrown exceptions — trip a per-plugin circuit after
`PLUGIN_CIRCUIT_THRESHOLD` (default 10). While open, the plugin is skipped entirely.
The forum stays up; the plugin stops running.

## 12. Troubleshooting

**The widget does not appear.** Almost always the element name. The server emits
`ferum-slot-{id}-{slot}` with non-alphanumerics folded to `-`; your
`customElements.define` must use that exact string. Check the rendered HTML source and
compare. An undefined custom element produces no console error.

**The widget appears on the wrong pages, or not on the homepage.** `content_before`
and `content_after` live in `base.html` and therefore appear everywhere;
`home_feed_top` is homepage-only. If you are on a custom theme that overrides
`home.html`, that template must render `plugin_slots.home_feed_top` itself.

**Editing config changes nothing.** You are reading `props`, which are frozen at
activation. Deactivate and reactivate to rewrite them, or switch to the
config-over-RPC pattern in `examples/plugins/home-hero/`.

**A before-hook never blocks anything.** Three candidates, in order: the hook is not
declared in `[capabilities].hooks`; the admin did not grant it; or the handler is
throwing and being treated as allow. Check **Admin → Plugins → *plugin* → Logs**,
which is where exceptions and timeouts are recorded.

**`SyntaxError` on install or activation.** The sandbox is `boa_engine`, not Node and
not a browser. `import`, `require`, `async`/`await`, `Promise`, `fetch` and
`setTimeout` are all unavailable server-side. Bundle to a single file of synchronous
ES5-compatible code.

**The circuit opened.** Ten consecutive failures. Fix the cause — usually a hook
exceeding `PLUGIN_HOOK_TIMEOUT_MS` because it makes a slow outbound call — then
deactivate and reactivate the plugin.

**`Ferum.db.query` fails on a core table.** It is supposed to. Plugin SQL runs as the
`ferum_plugin` role with no grants outside `plugin_{slug}`.

**Tier 3 does nothing.** It is unimplemented. See [section 1](#1-tiers).
