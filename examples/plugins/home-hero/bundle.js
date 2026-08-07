/**
 * Ferum Home Hero — Tier 2 Script Plugin (RPC + UI Slot)
 *
 * Replaces the masthead that used to be hard-coded into the ferum-review theme's
 * home.html. Everything a visitor reads — eyebrow, headline, sub-copy, both
 * calls to action, and the photo mosaic — comes from this plugin's config row,
 * so it is editable from Admin → Plugins → Home Hero → Configuration and is not
 * tied to any one theme.
 *
 * This file runs in two completely different JS environments, each guarded by a
 * `typeof` check so the wrong half never executes where its globals don't exist:
 *
 *   1. Server-side, inside boa_engine (no DOM, no `fetch`): registers the
 *      `get_content` handler on `__ferum_rpc`, reachable at
 *      POST /api/plugins/com.ferum.home-hero/rpc/get_content.
 *
 *   2. Client-side, in the browser (no `__ferum_rpc`, no `Ferum.*`): registers
 *      this plugin's custom element for the `home_feed_top` slot, which calls
 *      that RPC and renders the masthead. The tag carries the plugin slug (see
 *      TAG below), which is what lets another plugin sit in the same slot.
 *
 * WHY AN RPC AND NOT PROPS. [ui_slots.*].props are written into the
 * plugin_ui_slots row once, at activation, and `resolve_config_template`
 * ({{config.x}}) is applied only to webhook URLs — never to props. Content
 * carried in props would therefore need a deactivate/activate cycle after every
 * edit. Reading `Ferum.config` inside the RPC handler makes a config save take
 * effect on the next page load.
 *
 * WHAT THIS COSTS. The masthead is painted by JavaScript, so the headline and
 * the first image are not in the server's HTML: the browser's preload scanner
 * cannot start the hero image early, and the block has no height until the RPC
 * answers. That is the accepted trade of moving the hero out of the theme.
 */

// ─── Server-side: RPC handler ───────────────────────────────────────────────

if (typeof __ferum_rpc === 'object') {
    (function () {
        // The whole config object, every locale included. It is a few hundred
        // bytes and the client picks the branch it needs — cheaper than teaching
        // the RPC context about locales, which it does not carry.
        __ferum_rpc['get_content'] = function () {
            return { ok: true, data: Ferum.config || {} };
        };

        Ferum.log.info('Home Hero RPC handler registered');
    })();
}

// ─── Client-side: the masthead element ──────────────────────────────────────

(function () {
    'use strict';

    // Must equal `ui_slot_element_tag(meta.id, "home_feed_top")` on the server:
    // `ferum-slot-` + the slug and the slot name, each lowercased with every
    // non-alphanumeric run folded to one `-`. Named once because the guard and
    // the define below must agree — two literals that drift register the element
    // twice or never, and neither shows up as an error.
    var TAG = 'ferum-slot-com-ferum-home-hero-home-feed-top';

    if (typeof customElements === 'undefined' || customElements.get(TAG)) return;

    var RPC_URL = '/api/plugins/com.ferum.home-hero/rpc/get_content';

    // A 2×2 grid. More tiles than this stops being a masthead and starts
    // being a gallery, and the odd-count CSS below only covers 1–4.
    var MAX_TILES = 4;

    function escapeHtml(text) {
        var d = document.createElement('div');
        d.appendChild(document.createTextNode(text == null ? '' : String(text)));
        return d.innerHTML;
    }

    // Config is admin-entered text, so a URL out of it is input, not a
    // constant. Mirrors the server's `is_safe_external_link`: a site-relative
    // path, or an explicit http(s) origin. Rejecting everything else is what
    // keeps `javascript:` out of an href. A protocol-relative `//host` is
    // refused too — it reads as a path and behaves as an origin.
    function safeUrl(raw) {
        var u = (raw == null ? '' : String(raw)).trim();
        if (!u) return '';
        if (u.indexOf('//') === 0) return '';
        if (u.charAt(0) === '/') return u;
        if (/^https?:\/\//i.test(u)) return u;
        return '';
    }

    // Exact match first, then the base subtag, then the configured fallback,
    // then whatever is listed first — a locale added to the forum before it
    // is added here still renders something rather than nothing.
    function pickLocale(cfg) {
        var locales = (cfg && cfg.locales) || {};
        var names = Object.keys(locales);
        if (!names.length) return null;

        var lang = (document.documentElement.getAttribute('lang') || '').toLowerCase();
        if (lang && locales[lang]) return locales[lang];

        var base = lang.split('-')[0];
        if (base && locales[base]) return locales[base];

        if (cfg.default_locale && locales[cfg.default_locale]) {
            return locales[cfg.default_locale];
        }
        return locales[names[0]];
    }

    function ctaHtml(cta, className, beforeIcon, afterIcon) {
        if (!cta) return '';
        var label = (cta.label == null ? '' : String(cta.label)).trim();
        var href = safeUrl(cta.href);
        if (!label || !href) return '';
        return '<a href="' + escapeHtml(href) + '" class="' + className + '">' +
            (beforeIcon || '') + escapeHtml(label) + (afterIcon || '') +
            '</a>';
    }

    function tileHtml(tile, index) {
        var src = safeUrl(tile && tile.image_url);
        if (!src) return '';

        var caption = (tile.caption == null ? '' : String(tile.caption)).trim();
        var link = safeUrl(tile.link);

        // The first tile is the desktop LCP candidate; the rest defer.
        var loading = index === 0
            ? ' fetchpriority="high"'
            : ' loading="lazy"';

        // The visible caption is the link's accessible name, so the img
        // carries alt="" — repeating it there makes a screen reader announce
        // the tile twice. A linked tile with no caption has no such name and
        // gets an explicit aria-label instead; an unlabelled link is a WCAG
        // failure, not a cosmetic gap.
        var img = '<img src="' + escapeHtml(src) + '" alt=""' + loading + '>';
        var name = caption
            ? '<span class="fh-hero-tile-name">' + escapeHtml(caption) + '</span>'
            : '';

        if (!link) {
            // No destination: a plain figure, not an anchor that goes nowhere.
            return '<div class="fh-hero-tile">' +
                '<img src="' + escapeHtml(src) + '" alt="' + escapeHtml(caption) + '"' + loading + '>' +
                name +
                '</div>';
        }

        return '<a class="fh-hero-tile" href="' + escapeHtml(link) + '"' +
            (caption ? '' : ' aria-label="Featured image"') + '>' +
            img + name +
            '</a>';
    }

    /* Self-contained styling, prefixed `fh-` so it never collides with a
       theme's own rules. Every custom property is read with a fallback: on
       ferum-review the theme's tokens are present and the masthead looks native
       to it, and on a theme that defines none of them the fallbacks are
       Bootstrap variables that exist everywhere.

       Colours for the primary button are deliberately NOT set here — it wears
       `btn btn-primary btn-accent` and lets the cascade decide, so a theme
       restyling its CTA restyles this one too. Setting them here would win over
       the theme (this <style> is injected into the document after theme.css)
       and freeze the button at whatever this file happened to guess. */
    var STYLE = '' +
        '.fh-hero{padding:.5rem 0 2rem;margin-bottom:1.5rem;' +
        '  border-bottom:1px solid var(--bs-border-color);}' +
        '.fh-hero-eyebrow{display:inline-block;margin:0 0 .9rem;padding-bottom:.5rem;' +
        '  border-bottom:2px solid var(--fr-accent,var(--bs-primary));' +
        '  font-size:var(--fr-label-size,.75rem);font-weight:700;' +
        '  letter-spacing:var(--fr-label-tracking,.08em);text-transform:uppercase;' +
        '  color:var(--fr-accent-text,var(--bs-primary));}' +
        '.fh-hero-title{margin:0 0 .8rem;max-width:22ch;' +
        '  font-size:clamp(1.6rem,1.05rem + 2.3vw,2.55rem);font-weight:750;' +
        '  letter-spacing:-.038em;line-height:1.08;' +
        '  color:var(--bs-emphasis-color);text-wrap:balance;}' +
        '.fh-hero-sub{margin:0 0 1.5rem;max-width:58ch;font-size:1rem;line-height:1.65;' +
        '  color:var(--bs-secondary-color);}' +
        '.fh-hero-cta{display:flex;align-items:center;flex-wrap:wrap;gap:.5rem 1.25rem;}' +
        /* inline-flex so min-height centres the label instead of stranding it
           on the first line of an over-tall button. NF-UX-02 wants 44px. */
        '.fh-hero-cta .btn{display:inline-flex;align-items:center;' +
        '  min-height:var(--fr-tap-target,44px);padding-inline:1.5rem;font-weight:600;}' +
        /* The secondary action is a LINK, not a second button: two filled
           controls of equal weight make the reader stop to work out which is
           the real one, and an outline button beside the primary overflows
           the copy column in the longer locale. */
        '.fh-hero-cta-link{display:inline-flex;align-items:center;' +
        '  min-height:var(--fr-tap-target,44px);font-size:.9375rem;font-weight:600;' +
        '  color:var(--bs-secondary-color);text-decoration:none;' +
        '  transition:color .15s ease;}' +
        '.fh-hero-cta-link:hover,.fh-hero-cta-link:focus-visible{' +
        '  color:var(--fr-accent-text,var(--bs-primary));}' +
        '.fh-hero-cta-link .fa-arrow-right{transition:transform .15s ease;}' +
        '.fh-hero-cta-link:hover .fa-arrow-right{transform:translateX(3px);}' +
        '.fh-hero-copy{min-width:0;}' +
        '.fh-hero-gallery{display:grid;grid-template-columns:1fr 1fr;gap:.5rem;}' +
        /* An odd count would leave a hole in the grid; promote the odd tile to
           a full-width band so the mosaic always reads as a finished block. */
        '.fh-hero-gallery > .fh-hero-tile:only-child,' +
        '.fh-hero-gallery > .fh-hero-tile:nth-child(3):last-child{grid-column:1/-1;}' +
        '.fh-hero-tile{position:relative;display:block;aspect-ratio:4/3;overflow:hidden;' +
        '  border:1px solid var(--bs-border-color);' +
        '  border-radius:var(--bs-border-radius-lg,.5rem);' +
        '  background:var(--fr-surface-sunken,var(--bs-tertiary-bg));' +
        '  box-shadow:var(--bs-box-shadow-sm);}' +
        '.fh-hero-tile img{width:100%;height:100%;object-fit:cover;' +
        '  transition:transform .5s ease;}' +
        '.fh-hero-tile:hover img{transform:scale(1.05);}' +
        '.fh-hero-tile-name{position:absolute;inset:auto 0 0 0;padding:1.4rem .65rem .5rem;' +
        '  font-size:.75rem;font-weight:600;line-height:1.2;color:#fff;' +
        '  background:linear-gradient(to top,rgba(2,6,23,.74),rgba(2,6,23,0));' +
        '  white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}' +
        /* Desktop split. Both columns stretch to the same grid row so the copy
           and the mosaic share one top and one bottom edge by construction,
           and the gallery gets a DEFINITE height so `grid-auto-rows:1fr` has a
           real number to divide and object-fit can crop. Sized by content
           instead, the photographs set the row height and no vertical
           alignment can absorb the difference. */
        '@media (min-width:768px){' +
        '  .fh-hero--split{display:grid;grid-template-columns:minmax(0,1.35fr) minmax(0,1fr);' +
        '    gap:clamp(1.5rem,4vw,3rem);align-items:stretch;}' +
        '  .fh-hero--split .fh-hero-copy{display:flex;flex-direction:column;justify-content:center;}' +
        '  .fh-hero--split .fh-hero-gallery{height:clamp(18rem,30vw,26rem);grid-auto-rows:1fr;}' +
        '  .fh-hero--split .fh-hero-tile{aspect-ratio:auto;min-height:0;}' +
        /* Exactly two tiles is the one count where a full-height row side by
           side turns landscape photography into two narrow portraits. */
        '  .fh-hero--split .fh-hero-gallery:has(> .fh-hero-tile:nth-child(2):last-child){' +
        '    grid-template-columns:1fr;}' +
        '}' +
        /* On a phone the mosaic would be the whole first screen, pushing the
           feed — the reason people came — out of sight. The copy carries the
           masthead alone; the photography returns at 768px. */
        '@media (max-width:767.98px){.fh-hero-gallery{display:none;}}' +
        '@media (max-width:575.98px){' +
        '  .fh-hero{padding-bottom:1.25rem;margin-bottom:1rem;}' +
        '  .fh-hero-title{margin-bottom:.55rem;}' +
        '  .fh-hero-sub{margin-bottom:1rem;font-size:.9375rem;line-height:1.5;}' +
        '  .fh-hero-cta{flex-direction:column;align-items:stretch;gap:.25rem;}' +
        '  .fh-hero-cta .btn,.fh-hero-cta-link{justify-content:center;}' +
        '}' +
        '@media (prefers-reduced-motion:reduce){' +
        '  .fh-hero-tile img,.fh-hero-tile:hover img,' +
        '  .fh-hero-cta-link .fa-arrow-right,.fh-hero-cta-link:hover .fa-arrow-right{' +
        '    transition:none;transform:none;}' +
        '}';

    class HomeHero extends HTMLElement {
        connectedCallback() {
            var self = this;
            // No placeholder and no skeleton: an empty box that later grows a
            // masthead shifts the feed twice. It stays absent until there is
            // something real to draw.
            fetch(RPC_URL, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({}),
            })
                .then(function (r) { return r.json(); })
                .then(function (r) { self._render((r && r.data) || {}); })
                // A masthead is decoration around the feed. If the RPC is
                // unreachable the page below it is still the whole point of
                // the visit, so this fails silently rather than planting an
                // error banner at the top of the homepage.
                .catch(function () { self._renderNothing(); });
        }

        _renderNothing() {
            this.innerHTML = '';
            this.style.display = 'none';
        }

        _render(cfg) {
            var copy = pickLocale(cfg);
            var tiles = (cfg.tiles || [])
                .slice(0, MAX_TILES)
                .map(tileHtml)
                .filter(function (h) { return h !== ''; });

            // Nothing configured yet — a freshly installed plugin. Render
            // nothing at all rather than an empty bordered band.
            if (!copy && !tiles.length) {
                this._renderNothing();
                return;
            }
            copy = copy || {};

            var eyebrow = (copy.eyebrow == null ? '' : String(copy.eyebrow)).trim();
            var title = (copy.title == null ? '' : String(copy.title)).trim();
            var subtitle = (copy.subtitle == null ? '' : String(copy.subtitle)).trim();

            // BOTH classes, and the order in the attribute is irrelevant — CSS
            // source order decides. base.html loads bootstrap.min.css, then the
            // default theme, then the active one, so a theme that defines
            // .btn-accent (ferum-review does: amber fill, slate text) overrides
            // the --bs-btn-* variables .btn-primary set, and a theme that does
            // not is left with plain btn-primary in the operator's
            // site_config primary_color.
            //
            // Naming only btn-primary here was a visible regression: the
            // masthead this replaced used btn-accent, so moving it into a plugin
            // silently turned the amber CTA blue.
            var primary = ctaHtml(
                copy.primary_cta,
                'btn btn-primary btn-accent',
                '<i class="fa-solid fa-pen me-2" aria-hidden="true"></i>'
            );
            // Trailing arrow, matching every other "go somewhere" link in this
            // theme; it leans right on hover (see STYLE) unless the visitor has
            // asked for reduced motion.
            var secondary = ctaHtml(
                copy.secondary_cta,
                'fh-hero-cta-link',
                '',
                '<i class="fa-solid fa-arrow-right ms-2 fa-xs" aria-hidden="true"></i>'
            );

            var gallery = tiles.length
                ? '<div class="fh-hero-gallery">' + tiles.join('') + '</div>'
                : '';

            this.classList.add('d-block');
            this.innerHTML =
                '<style>' + STYLE + '</style>' +
                '<section class="fh-hero' + (tiles.length ? ' fh-hero--split' : '') + '">' +
                '  <div class="fh-hero-copy">' +
                (eyebrow ? '<p class="fh-hero-eyebrow">' + escapeHtml(eyebrow) + '</p>' : '') +
                (title ? '<h1 class="fh-hero-title">' + escapeHtml(title) + '</h1>' : '') +
                (subtitle ? '<p class="fh-hero-sub">' + escapeHtml(subtitle) + '</p>' : '') +
                (primary || secondary
                    ? '<div class="fh-hero-cta">' + primary + secondary + '</div>'
                    : '') +
                '  </div>' +
                gallery +
                '</section>';
        }
    }

    customElements.define(TAG, HomeHero);
})();
