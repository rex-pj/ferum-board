/**
 * Ferum Simple Chatbox — Tier 2 Script Plugin (RPC + UI Slot)
 *
 * This file is dual-purpose and runs in two completely different JS
 * environments, each guarded by a `typeof` check so the wrong half never
 * executes where its globals don't exist:
 *
 *   1. Server-side, inside boa_engine (no DOM, no `fetch`): registers
 *      `__ferum_rpc` handlers for `send_message` / `get_history`, called via
 *      POST /api/plugins/com.ferum.simple-chatbox/rpc/{action}.
 *
 *   2. Client-side, in the browser (no `__ferum_rpc`, no `Ferum.*`):
 *      registers the <ferum-slot-home-feed-top> custom element — a single
 *      shared, always-visible chat room embedded above the thread feed on the
 *      homepage only (not a floating corner bubble, not shown on every page),
 *      calling the RPC endpoint above via plain `fetch()`.
 *
 * Storage note: message history is kept as a single JSON array under the
 * "messages" key via Ferum.storage (durable, plugin-scoped KV — see
 * Ferum.storage.* in script_runtime.rs). This read-modify-write pattern is
 * NOT safe under highly concurrent writes (two messages sent in the same
 * instant can race and one can be lost) — fine for a reference example, not
 * a production chat backend.
 */

// ─── Server-side: RPC handlers ─────────────────────────────────────────────

if (typeof __ferum_rpc === 'object') {
    (function () {
        function getMaxHistory() {
            var n = Ferum.config && Ferum.config.max_history;
            return (typeof n === 'number' && n > 0) ? n : 50;
        }

        __ferum_rpc['send_message'] = function (ctx) {
            if (!ctx.actor_id) {
                return { ok: false, error: 'Please log in to send a message.' };
            }

            var payload = ctx.payload || {};
            var body = payload.body || {};
            var text = String(body.text || '').trim();

            if (!text) {
                return { ok: false, error: 'Message text is required' };
            }
            if (text.length > 500) {
                return { ok: false, error: 'Message must be 500 characters or fewer' };
            }

            var history = Ferum.storage.get('messages') || [];
            var message = {
                username: payload.actor_username || 'unknown',
                display_name: payload.actor_display_name || payload.actor_username || 'unknown',
                text: text,
                at: Ferum.utils.now(),
            };
            history.push(message);

            var max = getMaxHistory();
            if (history.length > max) {
                history = history.slice(history.length - max);
            }
            Ferum.storage.set('messages', history);

            return { ok: true, data: message };
        };

        __ferum_rpc['get_history'] = function () {
            return { ok: true, data: Ferum.storage.get('messages') || [] };
        };

        Ferum.log.info('Simple Chatbox RPC handlers registered');
    })();
}

// ─── Client-side: <ferum-slot-home-feed-top> widget ─────────────────────────

if (typeof customElements !== 'undefined' && !customElements.get('ferum-slot-home-feed-top')) {
    (function () {
        var RPC_BASE = '/api/plugins/com.ferum.simple-chatbox/rpc/';
        // Real chat feel requires seeing what other people type without a manual
        // reload. There's no push channel available to plugins (SSE is core-only),
        // so this polls at a fixed interval the whole time the room is on screen.
        var POLL_MS = 3000;

        function escapeHtml(text) {
            var d = document.createElement('div');
            d.appendChild(document.createTextNode(text));
            return d.innerHTML;
        }

        function getHistory() {
            return fetch(RPC_BASE + 'get_history', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({}),
            }).then(function (r) { return r.json(); });
        }

        // The RPC endpoint's failure shape is Ferum's standard API error,
        // { error: { code, message } } — not a plain string. Reading r.error
        // directly (instead of r.error.message) renders "[object Object]".
        function errorMessage(r) {
            if (!r || !r.error) return null;
            if (typeof r.error === 'string') return r.error;
            return r.error.message || 'Something went wrong.';
        }

        // Consistent-per-username colored avatar circle, so a busy room with many
        // people talking is easy to visually scan, not just a wall of identical text.
        function avatarColor(name) {
            var hash = 0;
            for (var i = 0; i < name.length; i++) {
                hash = (hash * 31 + name.charCodeAt(i)) & 0xffffffff;
            }
            var hue = Math.abs(hash) % 360;
            return 'hsl(' + hue + ',55%,42%)';
        }

        function formatTime(iso) {
            var d = new Date(iso);
            if (isNaN(d.getTime())) return '';
            var now = new Date();
            var sameDay = d.getFullYear() === now.getFullYear() &&
                d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
            var hh = String(d.getHours()).padStart(2, '0');
            var mm = String(d.getMinutes()).padStart(2, '0');
            if (sameDay) return hh + ':' + mm;
            return (d.getMonth() + 1) + '/' + d.getDate() + ' ' + hh + ':' + mm;
        }

        var STYLE = '' +
            '.sc-msg{display:flex;gap:.6rem;margin-bottom:.85rem;}' +
            '.sc-avatar{flex-shrink:0;width:2rem;height:2rem;border-radius:50%;display:flex;' +
            '  align-items:center;justify-content:center;color:#fff;font-size:.75rem;font-weight:700;}' +
            '.sc-msg-meta{font-size:.72rem;}' +
            '.sc-msg-meta strong{font-size:.8rem;}' +
            '.sc-msg-text{font-size:.85rem;word-break:break-word;}';

        class SimpleChatbox extends HTMLElement {
            connectedCallback() {
                // Embedded in the main content column (content_before slot) as a
                // normal card in document flow — a shared room everyone sees,
                // not a floating per-visitor toggle.
                this.classList.add('d-block', 'mb-3');
                this._timer = null;

                this.innerHTML =
                    '<style>' + STYLE + '</style>' +
                    '<div class="card shadow-sm">' +
                    '  <div class="card-header d-flex justify-content-between align-items-center py-2">' +
                    '    <strong><i class="fa-solid fa-comments me-2"></i>Community Chat</strong>' +
                    '    <span class="badge bg-success-subtle text-success-emphasis small">● live</span>' +
                    '  </div>' +
                    '  <div class="card-body p-3" style="min-height:9rem;max-height:24rem;overflow-y:auto;" data-messages>' +
                    '    <div class="text-muted small">Loading messages…</div>' +
                    '  </div>' +
                    '  <div class="card-footer p-2">' +
                    '    <form data-form class="d-flex gap-2">' +
                    '      <input type="text" class="form-control" maxlength="500" placeholder="Say something to everyone…" data-input>' +
                    '      <button type="submit" class="btn btn-primary">Send</button>' +
                    '    </form>' +
                    '    <div class="small text-danger mt-1" data-send-error style="display:none;"></div>' +
                    '  </div>' +
                    '</div>';

                this._messagesEl = this.querySelector('[data-messages]');
                var form = this.querySelector('[data-form]');
                var input = this.querySelector('[data-input]');
                var self = this;

                form.addEventListener('submit', function (e) {
                    e.preventDefault();
                    var text = input.value.trim();
                    if (!text) return;
                    input.value = '';
                    self._sendMessage(text);
                });

                this._poll();
                this._schedule();
            }

            disconnectedCallback() {
                if (this._timer) clearTimeout(this._timer);
            }

            _schedule() {
                var self = this;
                if (this._timer) clearTimeout(this._timer);
                this._timer = setTimeout(function () {
                    self._poll();
                    self._schedule();
                }, POLL_MS);
            }

            _poll() {
                var self = this;
                getHistory().then(function (r) {
                    self._renderMessages((r && r.data) || []);
                }).catch(function () {
                    self._messagesEl.innerHTML = '<div class="text-danger small">Failed to load messages.</div>';
                });
            }

            _renderMessages(history) {
                // Preserve the "was scrolled to bottom" feel across polling refreshes
                // instead of yanking the scrollbar back down while someone is reading
                // older messages.
                var wasAtBottom = this._messagesEl.scrollTop + this._messagesEl.clientHeight >= this._messagesEl.scrollHeight - 4;

                if (!history.length) {
                    this._messagesEl.innerHTML =
                        '<div class="text-muted small text-center py-2">' +
                        '<i class="fa-regular fa-comment-dots fa-lg mb-2 d-block opacity-50"></i>' +
                        'No messages yet — be the first to say hi.</div>';
                    return;
                }
                this._messagesEl.innerHTML = history.map(function (m) {
                    var name = m.display_name || 'unknown';
                    var initial = name.trim().charAt(0).toUpperCase() || '?';
                    return '' +
                        '<div class="sc-msg">' +
                        '  <div class="sc-avatar" style="background:' + avatarColor(name) + ';">' + escapeHtml(initial) + '</div>' +
                        '  <div class="flex-grow-1 min-w-0">' +
                        '    <div class="sc-msg-meta"><strong>' + escapeHtml(name) + '</strong> ' +
                        '      <span class="text-muted">' + escapeHtml(formatTime(m.at)) + '</span></div>' +
                        '    <div class="sc-msg-text">' + escapeHtml(m.text) + '</div>' +
                        '  </div>' +
                        '</div>';
                }).join('');

                if (wasAtBottom) {
                    this._messagesEl.scrollTop = this._messagesEl.scrollHeight;
                }
            }

            _sendMessage(text) {
                var self = this;
                var errorEl = this.querySelector('[data-send-error]');
                errorEl.style.display = 'none';
                fetch(RPC_BASE + 'send_message', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ text: text }),
                })
                    .then(function (r) { return r.json(); })
                    .then(function (r) {
                        var msg = errorMessage(r);
                        if (msg) {
                            errorEl.textContent = msg;
                            errorEl.style.display = 'block';
                            return;
                        }
                        self._poll();
                    })
                    .catch(function () {
                        errorEl.textContent = 'Failed to send — try again.';
                        errorEl.style.display = 'block';
                    });
            }
        }

        customElements.define('ferum-slot-home-feed-top', SimpleChatbox);
    })();
}
