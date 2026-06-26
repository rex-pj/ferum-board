# Announcement Banner — Ferum Plugin (Tier 2 Script + UI Slot)

Displays a configurable announcement banner at the top of every public page.
This plugin demonstrates how Ferum injects **server-side UI slots** into Tera
templates using vanilla-JS custom elements.

## Install

1. Upload `announcement-banner-1.0.0.fpkg` via **Admin → Plugins → Upload Plugin**.
2. Review the declared UI slot (`content_before`) and confirm install.
3. Click **Activate** on the plugin list page.
4. The banner is now visible on all public pages.

## How it works (architecture overview)

```
plugin.toml [ui_slots.content_before]
    │
    ▼ (at activation time)
plugin_ui_slots table row
  slot_name         = "content_before"
  custom_element_tag = "ferum-slot-content-before"
  asset_url         = "/plugins/com.ferum.announcement-banner/assets/bundle.js"
  props             = ['data-message="…"', 'data-kind="info"', …]
    │
    ▼ (on every page request)
plugin_ctx_data() helper in page_handler.rs
  → builds: <ferum-slot-content-before data-message="…" data-kind="info" data-dismissible="true">
            </ferum-slot-content-before>
  → injects into Tera context as plugin_slots.content_before
    │
    ▼
themes/default/templates/base.html
  {% if plugin_slots.content_before %}
    {% for slot in plugin_slots.content_before %}{{ slot.html | safe }}{% endfor %}
  {% endif %}
    │
    ▼ (in the browser)
bundle.js custom element upgrades <ferum-slot-content-before>
  → reads data-* attributes set at activation time
  → renders Bootstrap alert with optional dismiss button
```

## Updating the banner message

The banner text and style are embedded as HTML attributes at **activation time**
(read from `[ui_slots.content_before].props` in `plugin.toml`). To change the
text without editing the TOML:

1. Deactivate the plugin.
2. Edit the `data-message` prop in `plugin.toml` and repackage as a new `.fpkg`.
3. Install and activate the new version.

For fully dynamic config (change message via admin UI with no reinstall), a
future version of the plugin could call `GET /api/plugins/:slug/config` from
`bundle.js` and render content client-side after fetching.

## Available slots in the default theme

| Slot name        | Custom element tag               | Location in base.html           |
|------------------|----------------------------------|---------------------------------|
| `content_before` | `ferum-slot-content-before`      | Above `{% block content %}`     |
| `content_after`  | `ferum-slot-content-after`       | Below `{% block content %}`     |
| `navbar_end`     | `ferum-slot-navbar-end`          | Right side of the top navbar    |

Custom themes may add additional slots by following the same pattern.

## Config fields

| Field         | Required | Default          | Description                              |
|---------------|----------|------------------|------------------------------------------|
| `message`     | ✓        | —                | Banner text (plain text)                 |
| `kind`        |          | `info`           | Bootstrap alert variant                  |
| `dismissible` |          | `true`           | Allow visitors to close the banner       |
