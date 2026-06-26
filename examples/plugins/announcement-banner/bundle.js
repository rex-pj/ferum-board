/**
 * Ferum Announcement Banner — Tier 2 Script Plugin (UI Slot: content_before)
 *
 * Registers the <ferum-slot-content-before> custom element.
 * Ferum injects this element server-side into every public page via the
 * plugin_slots.content_before Tera context variable.
 *
 * Configuration is read from HTML data-* attributes set at plugin activation
 * time (see [ui_slots.content_before].props in plugin.toml):
 *
 *   data-message      — banner text (plain text, required)
 *   data-kind         — Bootstrap alert variant (default: "info")
 *   data-dismissible  — "true" | "false" (default: "true")
 *
 * Dismissed state persists for the browser session via sessionStorage so the
 * user is not shown the banner again after closing it.
 */
(function () {
    'use strict';

    var DISMISS_KEY = 'ferum-banner-dismissed';

    class AnnouncementBanner extends HTMLElement {
        connectedCallback() {
            var message     = (this.dataset.message     || '').trim();
            var kind        = (this.dataset.kind        || 'info').trim();
            var dismissible = this.dataset.dismissible !== 'false';

            // Nothing to show.
            if (!message) {
                this.style.display = 'none';
                return;
            }

            // User already dismissed this session.
            if (dismissible && sessionStorage.getItem(DISMISS_KEY) === message) {
                this.style.display = 'none';
                return;
            }

            this.setAttribute('role', 'alert');

            var closeBtn = dismissible
                ? '<button type="button" class="btn-close" aria-label="Close"></button>'
                : '';

            this.innerHTML =
                '<div class="alert alert-' + kind +
                (dismissible ? ' alert-dismissible' : '') +
                ' rounded-0 mb-0 border-top-0 border-start-0 border-end-0">' +
                '<div class="container-fluid d-flex align-items-center gap-2">' +
                '<i class="fa-solid fa-bullhorn flex-shrink-0"></i>' +
                '<span class="flex-grow-1">' + this._escapeHtml(message) + '</span>' +
                closeBtn +
                '</div>' +
                '</div>';

            if (dismissible) {
                var btn = this.querySelector('.btn-close');
                if (btn) {
                    btn.addEventListener('click', function () {
                        sessionStorage.setItem(DISMISS_KEY, message);
                        this.closest('ferum-slot-content-before').style.display = 'none';
                    }.bind(btn));
                }
            }
        }

        _escapeHtml(text) {
            var d = document.createElement('div');
            d.appendChild(document.createTextNode(text));
            return d.innerHTML;
        }
    }

    if (!customElements.get('ferum-slot-content-before')) {
        customElements.define('ferum-slot-content-before', AnnouncementBanner);
    }
})();
