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
  custom_element_tag = "ferum-slot-com-ferum-announcement-banner-content-before"
  asset_url         = "/plugins/com.ferum.announcement-banner/assets/bundle.js"
  props             = ['data-message="…"', 'data-kind="info"', …]
    │
    ▼ (on every page request)
plugin_ctx_data() helper in handlers/pages/mod.rs
  → builds: <ferum-slot-com-ferum-announcement-banner-content-before data-message="…" data-kind="info" data-dismissible="true">
            </ferum-slot-com-ferum-announcement-banner-content-before>
  → injects into Tera context as plugin_slots.content_before
    │
    ▼
themes/default/templates/base.html
  {% if plugin_slots.content_before %}
    {% for slot in plugin_slots.content_before %}{{ slot.html | safe }}{% endfor %}
  {% endif %}
    │
    ▼ (in the browser)
bundle.js custom element upgrades <ferum-slot-com-ferum-announcement-banner-content-before>
  → reads data-* attributes set at activation time
  → renders Bootstrap alert with optional dismiss button
```

## Updating the banner message

The banner text and style are embedded as HTML attributes at **activation time**,
read from `[ui_slots.content_before].props`. Editing the plugin's config in the
admin UI does **not** change them — props are written into the `plugin_ui_slots`
row once and never re-resolved. So:

1. Deactivate the plugin.
2. Edit the `data-message` prop in `plugin.toml` and repackage as a new `.fpkg`.
3. Install and activate the new version.

For content that changes after install, copy `examples/plugins/home-hero`
instead: declare no props, expose an RPC action that returns `Ferum.config`, and
fetch it from `bundle.js` on connect. An admin config save then takes effect on
the next page load. This plugin keeps props deliberately — it is the smallest
demonstration of them, and of their one real limitation.

## Slot names and element names

The custom element name is derived by `ui_slot_element_tag(slug, slot_name)`:
`ferum-slot-` + the plugin slug + the slot name, each lowercased with every
non-alphanumeric run folded to a single `-`. **The slug is part of it**, which is
what lets two plugins occupy the same slot — they get different elements and
render in `load_order` order.

`bundle.js` must define exactly that name; here it is the `TAG` constant at the
top. `tests/domain/src/models/plugin.rs` reads every example manifest and asserts
its bundle mentions the tag the server will emit, so the two cannot drift apart
silently.

| Slot name        | This plugin's element                                     | Location in base.html       |
|------------------|-----------------------------------------------------------|-----------------------------|
| `content_before` | `ferum-slot-com-ferum-announcement-banner-content-before` | Above `{% block content %}` |
| `content_after`  | *(same rule, `-content-after` suffix)*                    | Below `{% block content %}` |
| `navbar_end`     | *(same rule, `-navbar-end` suffix)*                       | Right of the top navbar     |

`home_feed_top` (homepage only) and `sidebar_left_top` also exist. Custom themes
may add more by rendering `plugin_slots.<name>` the same way.

## Config fields

| Field         | Required | Default          | Description                              |
|---------------|----------|------------------|------------------------------------------|
| `message`     | ✓        | —                | Banner text (plain text)                 |
| `kind`        |          | `info`           | Bootstrap alert variant                  |
| `dismissible` |          | `true`           | Allow visitors to close the banner       |
