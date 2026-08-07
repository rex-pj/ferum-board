# Home Hero

An admin-editable homepage masthead: eyebrow, headline, sub-copy, two calls to
action and a photo mosaic. Injected into the `home_feed_top` slot, so it is
independent of the active theme.

This replaces the masthead that used to be hard-coded into the `ferum-review`
theme's `home.html`, where the headline borrowed `site.tagline` — a field that is
also the page's `<meta name="description">` and its Open Graph description, so
writing a headline there meant writing a bad meta description.

## Install

```powershell
pwsh scripts/package-examples.ps1        # builds home-hero-1.0.0.fpkg
```

Admin → Plugins → Upload `home-hero-1.0.0.fpkg` → Install → Activate.

Requires a build with the `script_plugins` feature (on by default; **off** under
`cargo build --no-default-features`). Without it the plugin will not activate and
the homepage simply has no masthead.

## Configuration

Admin → Plugins → Home Hero → Configuration. The editor is a raw JSON textarea,
with a **Fields** reference below it rendered from `[config_schema]` in
`plugin.toml` — that is where an operator who never opens this file learns the
rules, including the two under "Two things that will bite you". Keep the two in
step: a constraint added here and not there reaches nobody.

Paste an object of this shape:

```json
{
  "default_locale": "vi",
  "locales": {
    "vi": {
      "eyebrow": "ĐÁNH GIÁ VẬT LIỆU & NỘI THẤT",
      "title": "Đánh giá thật từ người đã thi công",
      "subtitle": "Kiến trúc sư, nhà thầu và chủ nhà ghi lại những con số quan trọng: độ bền theo thời gian, khả năng chống ẩm, độ khó khi thi công và chi phí thực tế trên mỗi m².",
      "primary_cta":   { "label": "Viết đánh giá",     "href": "/new-thread" },
      "secondary_cta": { "label": "Danh mục sản phẩm", "href": "/catalog" }
    },
    "en": {
      "eyebrow": "MATERIAL & FURNITURE REVIEWS",
      "title": "Real reviews from the people who built it",
      "subtitle": "Architects, contractors and homeowners log the numbers that matter: how it wears, how it handles damp, how hard it was to fit, and what it actually cost per m².",
      "primary_cta":   { "label": "Write a review",   "href": "/new-thread" },
      "secondary_cta": { "label": "Product catalog",  "href": "/catalog" }
    }
  },
  "tiles": [
    { "image_url": "/files/hero/ab12…", "caption": "Oak flooring, 3 years in", "link": "/catalog/san-go-soi" },
    { "image_url": "/files/hero/cd34…", "caption": "Milano leather sofa",      "link": "/catalog/sofa-da-milano" }
  ]
}
```

Saving takes effect on the **next page load** — no deactivate/activate cycle.
(That is why content is read through the `get_content` RPC rather than carried in
`[ui_slots.*].props`, which are written once at activation.)

### Locale selection

The element reads `<html lang>` and looks for an exact match, then the base
subtag (`en-US` → `en`), then `default_locale`, then whichever key is listed
first. A locale the forum serves but this config does not name still renders
something.

### Fields

Every field is optional; anything absent is simply not drawn.

| Field | Notes |
|---|---|
| `eyebrow` | Short uppercase label above the headline |
| `title` | Rendered as the page's `<h1>` |
| `subtitle` | One paragraph, capped at 58ch by the stylesheet |
| `primary_cta` | `{ label, href }` — the filled button |
| `secondary_cta` | `{ label, href }` — a text link, deliberately not a second button |
| `tiles` | Up to 4. Extra entries are ignored. `{ image_url, caption, link }`; `link` may be omitted, and the tile then renders as a plain figure |

`href`, `link` and `image_url` must be site-relative (`/…`) or an explicit
`http(s)://` URL. Anything else — including protocol-relative `//host` — is
dropped, which is what keeps `javascript:` out of an `href`.

## Two things that will bite you

**1. `image_url` must be same-origin or on your CDN.** The
Content-Security-Policy sets `img-src 'self' data: blob:` plus the configured
storage origin. An image pasted from an external host (imgur, a stock library)
is blocked by the browser: no server error, nothing in the logs, just a blank
tile and a console violation.

To get a usable URL, upload the image somewhere the forum already stores files —
attach it to a post, or reuse a product photo — and copy its `/files/<key>` path.

**2. Nothing reference-counts these images.** Plugin config is not part of the
CAS bookkeeping. If the `/files/<key>` you paste belongs to a post attachment and
that post is later deleted, the file's `ref_count` reaches zero, the
`GcStorageKey` job removes the blob, and the tile breaks — with no warning
anywhere. Prefer a key that something durable also references, and re-upload if a
tile goes blank.

## Sharing the slot

`simple-chatbox` uses `home_feed_top` too, and both can be active at once. The
custom element name is derived by `ui_slot_element_tag(slug, slot_name)` and
carries the plugin slug, so each plugin owns its own element:

```
ferum-slot-com-ferum-home-hero-home-feed-top
ferum-slot-com-ferum-simple-chatbox-home-feed-top
```

`bundle.js` must define exactly that name — it is in the `TAG` constant at the
top of the client-side half, and `every_example_bundle_defines_the_tag_the_server_will_emit`
fails the build if the two ever disagree.

Widgets in one slot render in `load_order` order, adjustable per placement with
`PATCH /api/admin/plugins/:slug/ui-slots/:slot_id`. Every plugin's first slot is
seeded at 100, so until one is changed the order falls back to plugin slug,
alphabetically — predictable, but probably not what you want if you care which
comes first.

Themes must render the slot for any of this to appear. `default` and
`ferum-review` do; `ferum-sumi` and `ferum-arcade` gained it in the same change
that added this plugin.

## What it costs

The masthead is painted by JavaScript. The headline and the first image are not
in the server's HTML, so the preload scanner cannot start the hero image early
and the block has no height until the RPC answers. That is the accepted trade for
having the hero live outside the theme; the alternative is a server-rendered
partial in core, which is not what a plugin can be today.
