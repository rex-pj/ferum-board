# Plugin Developer Guide

**Audience:** Developers building plugins for Ferum Board  
**Version:** 1.0.0

> **Implementation status (2026-06-26):**
> - **Tier 1 (Manifest)** — ✅ Implemented
> - **Tier 2 (Script / boa_engine)** — ✅ Implemented
> - **Tier 3 (Service / sidecar)** — 🔲 Planned; not yet implemented
> - **`ferum-plugin-cli`** — 🔲 Planned; the `ferum-plugin` CLI does not exist yet
> - **`@ferum-board/plugin-sdk`** — 🔲 Planned; the npm package does not exist yet
>
> Sections 5, 7.1–7.3 (Tier 3 tutorial, CLI install loop, migration testing) are **forward-looking design documentation** — do not use them as an implementation guide until Tier 3 ships.

---

## Table of Contents

1. [Before You Start](#1-before-you-start)
2. [Plugin Package Format](#2-plugin-package-format)
3. [Tutorial: Tier 1 — Manifest Plugin (Slack Notifications)](#3-tutorial-tier-1--manifest-plugin)
4. [Tutorial: Tier 2 — Script Plugin (AI Spam Filter)](#4-tutorial-tier-2--script-plugin)
5. [Tutorial: Tier 3 — Service Plugin (Thread Chatbox)](#5-tutorial-tier-3--service-plugin)
6. [Plugin Installation Guide (for Operators)](#6-plugin-installation-guide-for-operators)
7. [Testing Your Plugin](#7-testing-your-plugin)
8. [Troubleshooting](#8-troubleshooting)
9. [Reference: plugin.toml Fields](#9-reference-plugintoml-fields)
10. [Reference: Available Hooks & Events](#10-reference-available-hooks--events)
11. [Reference: Plugin Query API](#11-reference-plugin-query-api)

---

## 1. Before You Start

### 1.1 Choose the Right Tier

| I want to... | Use |
|---|---|
| Send forum events to Slack / Discord / Zapier | **Tier 1** |
| Show a config form in admin settings | **Tier 1** |
| Block spam posts before they are saved | **Tier 2** |
| Transform post content (auto-link, syntax highlight) | **Tier 2** |
| Add a real-time chatbox to threads | **Tier 3** |
| Replace the search engine | **Tier 3** |
| Build an analytics dashboard | **Tier 3** |
| Implement SSO / LDAP login | **Tier 3** |

### 1.2 Tools You Need

**All tiers:**
- Any text editor or IDE
- `ferum-plugin-cli` — the official packaging tool

```bash
npm install -g @ferum-board/plugin-cli
# or
cargo install ferum-plugin-cli
```

**Tier 2 additionally:**
- Node.js 20+ (for TypeScript compilation)
- `@ferum-board/plugin-sdk` package

**Tier 3 additionally:**
- Your language of choice (Go, Rust, Python, Node.js, etc.)
- An HTTP server library

### 1.3 Development Setup

Run a local forum instance with plugins enabled:

```bash
# In your forum's .env
PLUGINS_DIR=./plugins
PLUGIN_INTERNAL_SECRET=dev-plugin-secret-not-for-production
```

The forum auto-discovers and hot-reloads Tier 1 plugins from `PLUGINS_DIR` in development mode. Tier 2 and 3 require a re-install via admin UI after changes.

---

## 2. Plugin Package Format

Every plugin is distributed as a `.fpkg` file — a standard ZIP archive renamed to `.fpkg`.

### 2.1 Directory Structure

```
your-plugin.fpkg   (ZIP archive containing:)
├── plugin.toml              ← REQUIRED: plugin manifest
├── icon.png                 ← optional: 128×128px, shown in admin UI
├── README.md                ← optional: shown in plugin detail page
│
├── hooks/                   ← Tier 2 only: JS hook files
│   ├── before_post_create.ts
│   └── after_thread_created.ts
│
├── dist/                    ← compiled output
│   ├── bundle.js            ← Tier 2: pre-bundled hook scripts
│   ├── thread-chatbox.js    ← Tier 3: Web Component for UI slot
│   └── admin-widget.js      ← Tier 3: admin panel component
│
├── bin/                     ← Tier 3 only: service binary/scripts
│   └── server               ← executable (or server.js, server.py, etc.)
│
└── migrations/              ← Tier 3 only (when db_schema = true)
    ├── 001_create_rooms.sql
    └── 002_create_messages.sql
```

### 2.2 Build and Package

```bash
# Scaffold a new plugin
ferum-plugin new my-plugin --tier=2

# Validate manifest
ferum-plugin validate

# Build (compiles TypeScript, bundles assets)
ferum-plugin build

# Create .fpkg archive
ferum-plugin pack
# → outputs: my-plugin-1.0.0.fpkg
```

---

## 3. Tutorial: Tier 1 — Manifest Plugin

**Goal:** Send a Slack message whenever a new thread is created.

This is the simplest possible plugin — no code, just configuration.

### Step 1: Create plugin.toml

```toml
[meta]
id          = "com.example.slack-new-threads"
name        = "Slack: New Thread Alerts"
version     = "1.0.0"
min_ferum   = "2.0.0"
tier        = "manifest"
author      = "Your Name"
description = "Posts a message to Slack when a new thread is created"

[capabilities]
webhooks  = true
admin_panels = ["settings"]

[config_schema]
type     = "object"
required = ["slack_webhook_url", "slack_channel"]

[config_schema.properties.slack_webhook_url]
type        = "string"
title       = "Slack Incoming Webhook URL"
description = "Create one at api.slack.com/apps → Incoming Webhooks"
format      = "uri"

[config_schema.properties.slack_channel]
type    = "string"
title   = "Channel Name"
default = "#forum-alerts"

[config_schema.properties.include_content_preview]
type    = "boolean"
title   = "Include content preview in message"
default = true

# ─── Webhook declarations ─────────────────────────────────────────────────────
[[webhooks]]
label  = "Notify Slack: new thread"
event  = "thread.created"
url    = "{{config.slack_webhook_url}}"

[[webhooks]]
label  = "Notify Slack: user banned"
event  = "user.banned"
url    = "{{config.slack_webhook_url}}"
```

### Step 2: Package it

```bash
ferum-plugin validate   # check manifest is correct
ferum-plugin pack       # → slack-new-threads-1.0.0.fpkg
```

That's it. No code needed. The forum resolves `{{config.slack_webhook_url}}` at dispatch time using whatever URL the admin typed into the config form.

### Step 3: What the admin sees

After installation, the admin fills in:
- **Slack Incoming Webhook URL** — `https://hooks.slack.com/services/T.../B.../...`
- **Channel Name** — `#forum-alerts`
- **Include content preview** — ✓

The forum then automatically registers two webhooks pointing to that URL.

---

## 4. Tutorial: Tier 2 — Script Plugin

**Goal:** Score each post with an external spam API before it is saved. Deny posts with a score above 0.9.

### Step 1: Scaffold

```bash
ferum-plugin new ai-spam-filter --tier=2
cd ai-spam-filter
npm install
```

Project structure created:

```
ai-spam-filter/
├── plugin.toml
├── hooks/
│   └── before_post_create.ts
├── package.json
└── tsconfig.json
```

### Step 2: Write plugin.toml

```toml
[meta]
id          = "com.example.ai-spam-filter"
name        = "AI Spam Filter"
version     = "1.0.0"
min_ferum   = "2.0.0"
tier        = "script"
author      = "Your Name"
description = "Scores post content via an external API and blocks spam"

[capabilities]
outbound_http = true
http_allowlist = ["api.akismet.com", "api.cleantalks.net"]
admin_panels  = ["settings"]
cache_quota_kb = 256

hooks = [
  { name = "before_post_create", priority = 50 },
]

[script]
entry_hook_dir = "hooks/"
timeout_ms     = 500
bundle_file    = "dist/bundle.js"

[config_schema]
type     = "object"
required = ["api_key", "api_endpoint"]

[config_schema.properties.api_key]
type  = "string"
title = "Spam API Key"

[config_schema.properties.api_endpoint]
type    = "string"
title   = "Spam API Endpoint"
default = "https://api.akismet.com/1.1/comment-check"

[config_schema.properties.block_threshold]
type    = "number"
title   = "Block threshold (0.0 – 1.0)"
default = 0.9
minimum = 0.0
maximum = 1.0
```

### Step 3: Write the hook

```typescript
// hooks/before_post_create.ts
import { defineHook } from '@ferum-board/plugin-sdk';
import type { BeforePostCreateContext } from '@ferum-board/plugin-sdk';

export default defineHook('before_post_create', async (ctx: BeforePostCreateContext) => {
  const { content_md, author_id, author_trust_level } = ctx.payload;

  // Trust level 3+ (Regular) — skip scoring, they are verified
  if (author_trust_level >= 3) {
    return ctx.allow();
  }

  // Check cache first — avoid scoring the same content twice
  const cacheKey = `spam:${hashContent(content_md)}`;
  const cached = await Ferum.cache.get(cacheKey);
  if (cached !== null) {
    const score = parseFloat(cached);
    return score >= Ferum.config.block_threshold
      ? ctx.deny('spam_detected', 'Your post was flagged as spam.')
      : ctx.allow();
  }

  // Call external spam API
  let score: number;
  try {
    const response = await Ferum.http.post(
      Ferum.config.api_endpoint,
      {
        api_key:      Ferum.config.api_key,
        content:      content_md,
        content_type: 'forum-post',
      },
      { 'Content-Type': 'application/json' }
    );
    score = response.spam_score ?? 0;
  } catch (err) {
    // API call failed — log and allow (fail-open for availability)
    Ferum.log.warn(`Spam API call failed: ${err.message}. Allowing post.`);
    return ctx.allow();
  }

  // Cache result for 10 minutes
  await Ferum.cache.set(cacheKey, String(score), 600);

  if (score >= Ferum.config.block_threshold) {
    Ferum.log.info(`Post blocked. Score: ${score}. Author: ${author_id}`);
    return ctx.deny('spam_detected', 'Your post was flagged as spam. Please revise it.');
  }

  return ctx.allow();
});

function hashContent(content: string): string {
  // Simple hash for cache key — provided by SDK
  return Ferum.utils.sha256(content).slice(0, 16);
}
```

### Step 4: SDK Reference for Tier 2

```typescript
// Everything available inside a hook script:

// Config values (read-only, set by admin in config form)
Ferum.config.api_key              // string
Ferum.config.block_threshold      // number

// Outbound HTTP (only to domains in http_allowlist)
await Ferum.http.get(url, headers?)
await Ferum.http.post(url, body, headers?)

// Isolated key-value cache (plugin-namespaced, cannot see other plugins)
await Ferum.cache.get(key)                    // string | null
await Ferum.cache.set(key, value, ttl_secs)   // void
await Ferum.cache.del(key)                    // void

// Logging (goes to plugin_logs table, visible in admin UI)
Ferum.log.info(message, context?)
Ferum.log.warn(message, context?)
Ferum.log.error(message, context?)

// Utilities
Ferum.utils.sha256(input)        // string
Ferum.utils.slugify(input)       // string
Ferum.utils.now()                // ISO 8601 timestamp string

// Hook response helpers
ctx.allow()
ctx.deny(error_code: string, message: string)
ctx.patch(fields: object)        // modify input fields before processing
```

### Step 5: Build and package

```bash
npm run build     # compiles TypeScript, bundles to dist/bundle.js
ferum-plugin pack # → ai-spam-filter-1.0.0.fpkg
```

---

## 5. Tutorial: Tier 3 — Service Plugin

**Goal:** Add a real-time chatbox to each thread page.

Tier 3 is the most capable tier. The plugin runs as a separate process and can own its own database, add API endpoints, and inject UI into forum pages.

This tutorial uses **Node.js**, but any language works.

### Step 1: Scaffold

```bash
ferum-plugin new thread-chatbox --tier=3 --lang=node
cd thread-chatbox
npm install
```

### Step 2: Write plugin.toml

```toml
[meta]
id          = "com.example.thread-chatbox"
name        = "Thread Chatbox"
version     = "1.0.0"
min_ferum   = "2.0.0"
tier        = "service"
author      = "Your Name"
description = "Real-time chat embedded in each thread"

[capabilities]
api_routes    = ["/api/plugins/chatbox/*"]
ui_slots      = ["thread.below_posts", "admin.dashboard.widget"]
admin_panels  = ["settings"]
db_schema     = true
listen_events = ["thread.created", "thread.deleted"]
permissions   = ["plugin.chatbox.moderate"]

[service]
command            = ["node", "bin/server.js"]
socket_path        = "/tmp/ferum-plugin-chatbox.sock"
startup_timeout_ms = 10000
health_interval_s  = 30
max_restart_count  = 3
restart_window_s   = 3600

[ui_slots."thread.below_posts"]
component          = "dist/thread-chatbox.js"
props              = ["threadId", "categorySlug", "currentUser"]
loading            = "lazy"

[ui_slots."admin.dashboard.widget"]
component          = "dist/admin-widget.js"
props              = []
loading            = "eager"

[db]
schema_name    = "plugin_chatbox"
migrations_dir = "migrations/"

[config_schema]
type     = "object"
required = ["max_message_length"]

[config_schema.properties.max_message_length]
type    = "integer"
title   = "Max message length (characters)"
default = 500

[config_schema.properties.messages_per_page]
type    = "integer"
title   = "Messages loaded per page"
default = 50

[config_schema.properties.moderation_mode]
type    = "string"
title   = "Moderation mode"
enum    = ["none", "post-moderation", "pre-moderation"]
default = "none"
```

### Step 3: Write database migrations

```sql
-- migrations/001_create_rooms.sql
CREATE TABLE rooms (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id  UUID NOT NULL,
    is_open    BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_rooms_thread ON rooms (thread_id);
```

```sql
-- migrations/002_create_messages.sql
CREATE TABLE messages (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id    UUID NOT NULL REFERENCES rooms ON DELETE CASCADE,
    user_id    UUID NOT NULL,
    username   TEXT NOT NULL,    -- denormalized for performance
    content    TEXT NOT NULL,
    is_deleted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_messages_room ON messages (room_id, created_at ASC)
    WHERE is_deleted = false;
```

### Step 4: Write the service

```javascript
// bin/server.js
const express = require('express');
const { createServer } = require('http');
const { Server: SocketIO } = require('socket.io');
const { Pool } = require('pg');
const fs = require('fs');

const app = express();
const httpServer = createServer(app);
const io = new SocketIO(httpServer);

// ─── Database ────────────────────────────────────────────────────────────────
// Connection string is injected by the forum as FERUM_PLUGIN_DB_URL
const db = new Pool({ connectionString: process.env.FERUM_PLUGIN_DB_URL });

// Set schema for all queries
db.on('connect', (client) => {
  client.query('SET search_path = plugin_chatbox, public');
});

// ─── Health check (required by forum) ────────────────────────────────────────
app.get('/health', (req, res) => {
  res.json({ status: 'ok', plugin: 'com.example.thread-chatbox' });
});

// ─── Forum event handler ──────────────────────────────────────────────────────
// The forum POSTs ForumEvents to this endpoint
app.post('/events', express.json(), async (req, res) => {
  const { event_type, payload } = req.body;

  if (event_type === 'thread.created') {
    // Auto-create a chat room for every new thread
    await db.query(
      'INSERT INTO rooms (thread_id) VALUES ($1) ON CONFLICT DO NOTHING',
      [payload.thread_id]
    );
  }

  if (event_type === 'thread.deleted') {
    // Clean up our data when a thread is deleted
    await db.query('DELETE FROM rooms WHERE thread_id = $1', [payload.thread_id]);
  }

  res.json({ ok: true });
});

// ─── Plugin Query API routes ──────────────────────────────────────────────────
// The forum proxies /api/plugins/chatbox/* here, with user context injected

app.get('/api/plugins/chatbox/rooms/:threadId', async (req, res) => {
  // Forum injects these headers after verifying user JWT
  const userId = req.headers['x-ferum-user-id'];
  const { threadId } = req.params;

  const { rows } = await db.query(
    'SELECT * FROM rooms WHERE thread_id = $1',
    [threadId]
  );

  if (rows.length === 0) {
    // Room doesn't exist yet — create it
    const { rows: newRoom } = await db.query(
      'INSERT INTO rooms (thread_id) VALUES ($1) RETURNING *',
      [threadId]
    );
    return res.json({ data: newRoom[0] });
  }

  res.json({ data: rows[0] });
});

app.get('/api/plugins/chatbox/rooms/:roomId/messages', async (req, res) => {
  const { roomId } = req.params;
  const limit = parseInt(req.query.limit) || 50;
  const before = req.query.before; // cursor-based pagination

  let query = 'SELECT * FROM messages WHERE room_id = $1 AND is_deleted = false';
  const params = [roomId];

  if (before) {
    query += ' AND created_at < $2';
    params.push(before);
  }

  query += ' ORDER BY created_at DESC LIMIT $' + (params.length + 1);
  params.push(limit);

  const { rows } = await db.query(query, params);
  res.json({ data: rows.reverse() });
});

app.post('/api/plugins/chatbox/rooms/:roomId/messages', express.json(), async (req, res) => {
  const userId = req.headers['x-ferum-user-id'];
  const username = req.headers['x-ferum-username'];
  const { roomId } = req.params;
  const { content } = req.body;

  if (!userId) return res.status(401).json({ error: 'Unauthenticated' });

  // Config injected by forum as FERUM_PLUGIN_CONFIG env var
  const config = JSON.parse(process.env.FERUM_PLUGIN_CONFIG || '{}');
  if (content.length > (config.max_message_length || 500)) {
    return res.status(400).json({ error: 'Message too long' });
  }

  const { rows } = await db.query(
    `INSERT INTO messages (room_id, user_id, username, content)
     VALUES ($1, $2, $3, $4) RETURNING *`,
    [roomId, userId, username, content]
  );

  const message = rows[0];

  // Broadcast via Socket.IO
  io.to(`room:${roomId}`).emit('message', message);

  res.status(201).json({ data: message });
});

// ─── WebSocket (Socket.IO) ────────────────────────────────────────────────────
io.on('connection', (socket) => {
  socket.on('join-room', (roomId) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave-room', (roomId) => {
    socket.leave(`room:${roomId}`);
  });
});

// ─── Start (Unix socket for forum communication) ──────────────────────────────
const SOCKET_PATH = process.env.FERUM_PLUGIN_SOCKET || '/tmp/ferum-plugin-chatbox.sock';

// Clean up old socket file
if (fs.existsSync(SOCKET_PATH)) fs.unlinkSync(SOCKET_PATH);

httpServer.listen(SOCKET_PATH, () => {
  fs.chmodSync(SOCKET_PATH, '660');
  console.log(`Chatbox plugin listening on ${SOCKET_PATH}`);
});
```

### Step 5: Write the Web Component (UI)

```javascript
// src/thread-chatbox.js  (compiled to dist/thread-chatbox.js)

class FerumChatboxThread extends HTMLElement {
  static get observedAttributes() {
    return ['threadid', 'categoryslug'];
  }

  connectedCallback() {
    this.threadId = this.getAttribute('threadid');
    this.currentUser = JSON.parse(this.getAttribute('currentuser') || 'null');
    this.render();
    this.loadRoom();
  }

  render() {
    this.innerHTML = `
      <div class="ferum-chatbox card mt-4">
        <div class="card-header d-flex align-items-center gap-2">
          <i class="fa-solid fa-comments"></i>
          <strong>Thread Chat</strong>
          <span class="badge bg-secondary ms-auto" id="chat-count">0</span>
        </div>
        <div class="card-body p-0">
          <div id="chat-messages" class="overflow-auto p-3" style="height:320px">
            <div class="text-center text-muted small py-4">Loading...</div>
          </div>
        </div>
        ${this.currentUser ? `
        <div class="card-footer">
          <div class="input-group">
            <input type="text" id="chat-input" class="form-control"
              placeholder="Type a message..." maxlength="500">
            <button class="btn btn-primary" id="chat-send">Send</button>
          </div>
        </div>
        ` : `
        <div class="card-footer text-center text-muted small">
          <a href="/login">Log in</a> to chat
        </div>
        `}
      </div>
    `;

    if (this.currentUser) {
      this.querySelector('#chat-send').addEventListener('click', () => this.sendMessage());
      this.querySelector('#chat-input').addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); this.sendMessage(); }
      });
    }
  }

  async loadRoom() {
    const res = await fetch(`/api/plugins/chatbox/rooms/${this.threadId}`);
    const { data: room } = await res.json();
    this.roomId = room.id;
    await this.loadMessages();
    this.connectWebSocket();
  }

  async loadMessages() {
    const res = await fetch(`/api/plugins/chatbox/rooms/${this.roomId}/messages?limit=50`);
    const { data: messages } = await res.json();
    this.renderMessages(messages);
  }

  renderMessages(messages) {
    const container = this.querySelector('#chat-messages');
    container.innerHTML = messages.map(m => `
      <div class="d-flex gap-2 mb-2">
        <strong class="text-nowrap small">${escapeHtml(m.username)}</strong>
        <span class="small">${escapeHtml(m.content)}</span>
        <span class="text-muted small ms-auto text-nowrap">
          ${new Date(m.created_at).toLocaleTimeString()}
        </span>
      </div>
    `).join('');
    container.scrollTop = container.scrollHeight;
  }

  async sendMessage() {
    const input = this.querySelector('#chat-input');
    const content = input.value.trim();
    if (!content) return;

    input.value = '';
    await fetch(`/api/plugins/chatbox/rooms/${this.roomId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content }),
    });
  }

  connectWebSocket() {
    // Socket.IO client loaded via CDN (declared in plugin.toml under [assets])
    const socket = io('/plugins/chatbox', { path: '/api/plugins/chatbox/socket.io' });
    socket.emit('join-room', this.roomId);
    socket.on('message', (message) => {
      const container = this.querySelector('#chat-messages');
      container.insertAdjacentHTML('beforeend', `
        <div class="d-flex gap-2 mb-2">
          <strong class="text-nowrap small">${escapeHtml(message.username)}</strong>
          <span class="small">${escapeHtml(message.content)}</span>
          <span class="text-muted small ms-auto text-nowrap">
            ${new Date(message.created_at).toLocaleTimeString()}
          </span>
        </div>
      `);
      container.scrollTop = container.scrollHeight;
    });
  }
}

function escapeHtml(str) {
  return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

customElements.define('ferum-chatbox-thread', FerumChatboxThread);
```

### Step 6: Build and package

```bash
npm run build     # bundles Web Components to dist/
ferum-plugin pack # → thread-chatbox-1.0.0.fpkg
```

---

## 6. Plugin Installation Guide (for Operators)

This section is for forum administrators installing plugins, not plugin developers.

### 6.1 Requirements

- You must have the `admin.plugins` permission (admin role by default)
- Ensure `PLUGINS_DIR` is set in your forum's environment and writable
- Ensure `PLUGIN_INTERNAL_SECRET` is set (different from `JWT_SECRET`)

### 6.2 Installing a Plugin

**Step 1:** Navigate to **Admin → Plugins**

**Step 2:** Click **Install Plugin** and choose one of:
- **Upload .fpkg file** — drag and drop or browse
- **Install from URL** — paste a direct download URL (e.g., from GitHub Releases)

**Step 3:** Review the capability screen carefully

The forum shows exactly what the plugin is asking for:

```
┌─────────────────────────────────────────────────────────┐
│  ⚠  Tier 3 Plugin — Review Required                     │
│                                                          │
│  Thread Chatbox v1.0.0                                   │
│  by Example Inc. · MIT License                           │
│                                                          │
│  This plugin requests the following capabilities:        │
│                                                          │
│  ✓ Run as a separate process on this server             │
│  ✓ Own a PostgreSQL schema: plugin_chatbox              │
│  ✓ Add API routes: /api/plugins/chatbox/*               │
│  ✓ Inject UI into: thread pages (below posts)           │
│  ✗ Make outbound HTTP calls                             │
│  ✗ Send webhooks                                        │
│                                                          │
│  You can revoke individual capabilities below:           │
│  [✓] Run as process    [✓] Own DB schema                │
│  [✓] API routes        [✓] UI injection                 │
│                                                          │
│  [Cancel]                      [Grant & Install →]      │
└─────────────────────────────────────────────────────────┘
```

> **Rule of thumb:** Only grant capabilities the plugin actually needs. If a Slack notification plugin asks to run a process — that's a red flag.

**Step 4:** Fill in the config form

Each plugin has a config form generated from its `config_schema`. Fill in required fields (marked with *).

**Step 5:** Click **Save & Activate**

The forum will:
1. Extract the plugin files to `PLUGINS_DIR/plugin-slug/`
2. Run database migrations (if any)
3. Seed new permission keys
4. Start the service process (Tier 3)
5. Register hooks and UI slots

### 6.3 Managing Installed Plugins

#### Disable a plugin (keeps data and config)

```
Admin → Plugins → [Plugin Name] → Disable
```

The plugin's hooks and UI slots are immediately deregistered. Data in `plugin_{slug}` schema is preserved. Re-enabling restores full functionality.

#### Update a plugin

```
Admin → Plugins → [Plugin Name] → Update → Upload new .fpkg
```

The forum will:
1. Validate the new version is compatible (`min_ferum` check)
2. Show a diff of capability changes
3. Run new migrations (additive only — migrations cannot be destructive)
4. Swap the process (Tier 3: graceful restart)

#### Uninstall a plugin

```
Admin → Plugins → [Plugin Name] → Uninstall
```

> ⚠️ **Warning:** Uninstalling a plugin with `db_schema = true` will **permanently delete** all data in `plugin_{slug}` schema. The forum will show a confirmation dialog listing what data will be lost.

```
┌────────────────────────────────────────────────────────────┐
│  ⚠  Confirm Uninstall                                      │
│                                                            │
│  This will permanently delete:                             │
│  • plugin_chatbox schema (3 tables, ~12,400 rows)          │
│  • Plugin config and settings                              │
│  • Plugin logs                                             │
│  • UI slot registrations                                   │
│                                                            │
│  Type the plugin name to confirm: [____________]           │
│                                                            │
│  [Cancel]                        [Confirm Uninstall]       │
└────────────────────────────────────────────────────────────┘
```

### 6.4 Viewing Plugin Logs

```
Admin → Plugins → [Plugin Name] → Logs
```

Logs are retained for 30 days. You can filter by:
- **Level:** trace / info / warn / error
- **Hook:** filter by hook name
- **Time range:** since / until

For Tier 3 plugins, you can also see process stdout/stderr in a separate **Process Output** tab.

### 6.5 Environment Variables for Plugin Operations

```bash
# Required
PLUGINS_DIR=/var/ferum/plugins       # where plugins are extracted
PLUGIN_INTERNAL_SECRET=<hex-32>      # different from JWT_SECRET

# Optional tuning
PLUGINS_HOOK_TIMEOUT_MS=500          # max time a hook can run
PLUGINS_CIRCUIT_THRESHOLD=10         # failures before circuit opens
PLUGINS_LOG_RETENTION_DAYS=30        # how long to keep plugin logs
```

---

## 7. Testing Your Plugin

### 7.1 Local Development Loop

```bash
# Terminal 1: Run the forum
cd /path/to/ferum-board/backend
cargo run

# Terminal 2: Run your Tier 3 service (watched mode)
cd my-plugin
npm run dev     # nodemon or equivalent

# Terminal 3: Pack and install via CLI (skips UI)
ferum-plugin pack
ferum-plugin install --forum-url=http://localhost:8080 \
                     --admin-token=<your-admin-jwt> \
                     --plugin=./my-plugin-1.0.0.fpkg
```

### 7.2 Testing Before-Hooks (Tier 2/3)

Use the hook test endpoint:

```bash
curl -X POST http://localhost:8080/api/admin/plugins/debug/hooks \
  -H "Authorization: Bearer <admin-jwt>" \
  -H "Content-Type: application/json" \
  -d '{
    "hook": "before_post_create",
    "payload": {
      "content_md": "Buy cheap watches at spam.com!!!",
      "author_id": "00000000-0000-0000-0000-000000000001",
      "author_trust_level": 0
    }
  }'

# Response:
{
  "decision": "deny",
  "reason": "Your post was flagged as spam.",
  "error_code": "spam_detected",
  "plugin": "com.example.ai-spam-filter",
  "duration_ms": 87
}
```

### 7.3 Testing DB Migrations (Tier 3)

```bash
# Apply migrations against a test database
ferum-plugin migrate --db-url=postgresql://localhost/ferum_test \
                     --schema=plugin_chatbox \
                     --migrations=./migrations/

# Verify schema was created
psql ferum_test -c "\dt plugin_chatbox.*"
```

### 7.4 Manifest Validation

Before packaging, always run:

```bash
ferum-plugin validate
```

This checks:
- `plugin.toml` schema correctness
- All referenced files exist (`component`, `bundle_file`, `command`)
- `config_schema` is valid JSON Schema
- Migration files are numbered correctly

---

## 8. Troubleshooting

### Plugin stuck in "Installing" state

The forum waits for the service process to respond to a health check. Check:

```bash
# Verify the socket path is accessible
ls -la /tmp/ferum-plugin-chatbox.sock

# Check process logs
Admin → Plugins → Thread Chatbox → Process Output

# Common causes:
# - Wrong socket path in plugin.toml vs. what the service is listening on
# - Service fails to start (missing dependency, wrong Node.js version)
# - startup_timeout_ms too low for slow machines
```

### Circuit breaker opened

```
Status: ERROR — Circuit open (10 failures in last hour)
```

This means the plugin's hook failed 10 times. To investigate:

```bash
# View recent errors
Admin → Plugins → AI Spam Filter → Logs → Filter: error

# Common causes:
# - External API (spam API) is down → fail-open is expected, but check API status
# - Hook timeout too low → increase timeout_ms in plugin.toml
# - Bug in hook script → fix and re-upload

# Reset circuit manually:
Admin → Plugins → AI Spam Filter → Reset Circuit
```

### Migrations failed on install

The migration runner will show which SQL file failed and why:

```
Migration 002_create_messages.sql failed:
ERROR: column "user_id" is of type uuid but expression is of type text

# Fix the SQL file, re-package, and re-install.
# Migrations already applied are not re-run.
```

### Hook is timing out

```
# In plugin logs:
WARN  before_post_create timed out after 500ms

# Options:
# 1. Optimize your hook code (cache more aggressively)
# 2. Increase timeout for this plugin in admin config:
Admin → Plugins → AI Spam Filter → Configure → Hook Timeout: 1000ms

# 3. If the external API is slow, add a local fallback:
try {
  score = await callExternalApi(content);
} catch (err) {
  // API slow or down — use local heuristic instead of failing
  score = localSpamHeuristic(content);
}
```

### Web Component not appearing on page

1. Check the plugin is **Active** and the slot is registered:
   ```bash
   curl http://localhost:8080/api/plugins/active-slots | jq '.["thread.below_posts"]'
   ```
2. Check browser console for `customElements.define` errors
3. Check the `custom_element_tag` in `plugin.toml` matches `customElements.define(...)` in your JS
4. Check CSP: the plugin asset URL must match `'self'` (it should, since it's served by the forum)

---

## 9. Reference: plugin.toml Fields

### `[meta]` — Required for all tiers

| Field | Type | Required | Description |
|---|---|:---:|---|
| `id` | string | ✓ | Reverse-domain unique ID: `com.example.my-plugin` |
| `name` | string | ✓ | Human-readable name |
| `version` | string | ✓ | SemVer: `1.0.0` |
| `min_ferum` | string | ✓ | Minimum forum version: `2.0.0` |
| `max_ferum` | string | | Optional upper bound |
| `tier` | enum | ✓ | `manifest` / `script` / `service` |
| `author` | string | | Author name or organization |
| `homepage` | string | | URL to plugin homepage |
| `license` | string | | e.g., `MIT`, `Apache-2.0` |
| `description` | string | | One-line summary |

### `[capabilities]` — Declare what your plugin needs

| Field | Type | Default | Tiers | Description |
|---|---|---|---|---|
| `webhooks` | bool | false | 1,2,3 | Register outgoing webhooks |
| `admin_panels` | string[] | [] | 1,2,3 | `["settings"]`, `["dashboard"]` |
| `outbound_http` | bool | false | 2,3 | Allow HTTP calls |
| `http_allowlist` | string[] | [] | 2,3 | Domains allowed for HTTP |
| `cache_quota_kb` | int | 256 | 2 | Max key-value cache size |
| `api_routes` | string[] | [] | 3 | Routes to mount: `["/api/plugins/slug/*"]` |
| `ui_slots` | string[] | [] | 3 | Slot names to inject into |
| `db_schema` | bool | false | 3 | Own a PostgreSQL schema |
| `listen_events` | string[] | [] | 1,2,3 | ForumEvents to receive |
| `permissions` | string[] | [] | 1,2,3 | New permission keys to seed |
| `hooks` | array | [] | 2,3 | Hook registrations (see below) |

**Hook registration:**
```toml
hooks = [
  { name = "before_post_create", priority = 50 },
  { name = "after_thread_created", priority = 100 },
]
```

Lower `priority` = runs earlier. Range: 50–999 (0–49 reserved for forum internals).

### `[script]` — Tier 2 only

| Field | Type | Required | Description |
|---|---|:---:|---|
| `entry_hook_dir` | string | ✓ | Directory containing hook `.ts` files |
| `timeout_ms` | int | 500 | Per-invocation timeout |
| `bundle_file` | string | ✓ | Pre-bundled output file path |

### `[service]` — Tier 3 only

| Field | Type | Required | Description |
|---|---|:---:|---|
| `command` | string[] | ✓ | Command to start the service |
| `socket_path` | string | ✓ | Unix socket path (must match what service binds to) |
| `startup_timeout_ms` | int | 5000 | Max time to wait for health check |
| `health_interval_s` | int | 30 | How often forum checks `/health` |
| `max_restart_count` | int | 3 | Crashes before circuit opens |
| `restart_window_s` | int | 3600 | Window for counting crashes |

### `[db]` — Tier 3 only (when `db_schema = true`)

| Field | Type | Required | Description |
|---|---|:---:|---|
| `schema_name` | string | ✓ | PostgreSQL schema: `plugin_chatbox` |
| `migrations_dir` | string | ✓ | Path to SQL migration files |

---

## 10. Reference: Available Hooks & Events

### Before-Hooks (can block the request)

| Hook Name | Triggered | Context Payload |
|---|---|---|
| `before_post_create` | Before a post is saved | `{ content_md, author_id, author_trust_level, thread_id, category_id }` |
| `before_thread_create` | Before a thread is created | `{ title, content_md, author_id, category_id, tags }` |
| `before_user_register` | Before registration is completed | `{ username, email }` — no password |
| `before_file_upload` | Before a file is stored | `{ filename, content_type, size_bytes, uploaded_by_id }` |

### After-Events (fire-and-forget, cannot block)

| Event Name | When | Payload |
|---|---|---|
| `post.created` | Post saved | `post_id, thread_id, thread_slug, author_id, category_id` |
| `post.deleted` | Post soft-deleted | `post_id, deleted_by_id` |
| `thread.created` | Thread created | `thread_id, thread_slug, author_id, category_id` |
| `thread.locked` | Thread locked | `thread_id, by_user_id` |
| `thread.moved` | Thread moved category | `thread_id, from_category, to_category, by_user_id` |
| `thread.deleted` | Thread deleted | `thread_id, deleted_by_id` |
| `reaction.added` | Reaction added | `post_id, user_id, kind, thread_id` |
| `reaction.removed` | Reaction removed | `post_id, user_id, kind` |
| `user.banned` | User banned | `user_id, by_user_id, reason, until` |
| `user.warned` | User warned | `user_id, by_user_id, reason` |
| `user.registered` | New user registered | `user_id, username` |
| `best_answer.marked` | Best answer set | `post_id, thread_id, by_user_id` |
| `trust_level.changed` | Trust level changed | `user_id, from, to` |

---

## 11. Reference: Plugin Query API

Tier 3 plugins can query forum data using a Plugin Service Token (automatically injected as the `FERUM_PLUGIN_TOKEN` environment variable, refreshed every 55 seconds).

### Authentication

```javascript
const token = process.env.FERUM_PLUGIN_TOKEN;

const res = await fetch('http://localhost:8080/api/plugins/_internal/threads/abc123', {
  headers: {
    'X-Ferum-Plugin-Token': token
  }
});
```

### Available Endpoints

| Endpoint | Scope | Returns |
|---|---|---|
| `GET /api/plugins/_internal/threads/:id` | `read:threads` | Thread metadata (no content) |
| `GET /api/plugins/_internal/threads?category=&page=` | `read:threads` | Paginated thread list |
| `GET /api/plugins/_internal/users/:id` | `read:users.public` | Public profile only |
| `GET /api/plugins/_internal/categories` | `read:categories` | Category tree |
| `GET /api/plugins/_internal/tags` | `read:tags` | Tag list |

> **Note:** These endpoints never return sensitive fields: `email`, `password_hash`, `ban_reason` (private), `failed_login_count`, etc. The scope system is enforced at the API level regardless of what scopes are in the token.

### Forum-Injected Request Headers (for proxied routes)

When a user calls your plugin's API routes (`/api/plugins/chatbox/*`), the forum validates the user's session and injects:

| Header | Value |
|---|---|
| `X-Ferum-User-Id` | User UUID (or absent if unauthenticated) |
| `X-Ferum-Username` | Username |
| `X-Ferum-Trust-Level` | `new` / `basic` / `member` / `regular` / `leader` |
| `X-Ferum-Is-Admin` | `true` / `false` |
| `X-Ferum-Permissions` | Comma-separated permission keys |

Your plugin should trust these headers — do not re-validate the JWT. The forum is the single source of auth truth.
