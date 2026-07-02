/**
 * Ferum Community Polls — Tier 2 Script Plugin (db + api + rpc + UI Slot)
 *
 * Demonstrates the three capabilities added alongside storage/rpc:
 *
 *   - db:  a real relational schema (`polls`, `poll_options`, `poll_votes`) in
 *          this plugin's own Postgres schema, queried via Ferum.db.query().
 *          Every SELECT/WITH statement here returns a single JSON/JSONB
 *          column — that's the contract the query() gateway expects.
 *   - api: Ferum.forum.createNotification() tells the poll's creator whenever
 *          someone votes, without the plugin ever touching the notifications
 *          table directly.
 *   - rpc: create_poll / vote / get_latest_poll are the plugin's own mini-API,
 *          called from the browser via POST /api/plugins/:slug/rpc/:action.
 *
 * Like simple-chatbox, this file is dual-purpose and runs in two different
 * JS environments, each guarded by a typeof check:
 *   1. Server-side in boa_engine (no DOM, no fetch): registers __ferum_rpc handlers.
 *   2. Client-side in the browser: registers <ferum-slot-sidebar-left-top>.
 */

// ─── Server-side: RPC handlers ─────────────────────────────────────────────

if (typeof __ferum_rpc === 'object') {
    (function () {
        function getMaxOptions() {
            var n = Ferum.config && Ferum.config.max_options;
            return (typeof n === 'number' && n > 0) ? n : 6;
        }

        __ferum_rpc['create_poll'] = function (ctx) {
            if (!ctx.actor_id) {
                return { ok: false, error: 'Please log in to create a poll.' };
            }

            var payload = ctx.payload || {};
            var body = payload.body || {};
            var question = String(body.question || '').trim();
            var rawOptions = Array.isArray(body.options) ? body.options : [];
            var options = rawOptions
                .map(function (o) { return String(o || '').trim(); })
                .filter(function (o) { return o.length > 0; });

            if (!question) {
                return { ok: false, error: 'Question is required' };
            }
            if (options.length < 2) {
                return { ok: false, error: 'At least 2 options are required' };
            }
            var maxOptions = getMaxOptions();
            if (options.length > maxOptions) {
                return { ok: false, error: 'Too many options (max ' + maxOptions + ')' };
            }

            // A writable CTE is the only valid way in Postgres to read an
            // INSERT's RETURNING back out as a FROM-subquery.
            var poll = Ferum.db.query(
                'WITH inserted AS (' +
                '  INSERT INTO polls (question, created_by, creator_username) VALUES ($1, $2, $3)' +
                '  RETURNING id, question, created_by, creator_username, created_at' +
                ') SELECT row_to_json(inserted) FROM inserted',
                [question, ctx.actor_id, payload.actor_username || 'unknown']
            );
            if (!poll || !poll.id) {
                return { ok: false, error: 'Failed to create poll' };
            }

            for (var i = 0; i < options.length; i++) {
                Ferum.db.query('INSERT INTO poll_options (poll_id, label) VALUES ($1, $2)', [poll.id, options[i]]);
            }

            Ferum.log.info('Poll created', { poll_id: poll.id, actor: ctx.actor_id });
            return { ok: true, data: poll };
        };

        __ferum_rpc['vote'] = function (ctx) {
            if (!ctx.actor_id) {
                return { ok: false, error: 'Please log in to vote.' };
            }

            var payload = ctx.payload || {};
            var body = payload.body || {};
            var pollId = String(body.poll_id || '');
            var optionId = String(body.option_id || '');
            if (!pollId || !optionId) {
                return { ok: false, error: 'poll_id and option_id are required' };
            }

            var already = Ferum.db.query(
                'SELECT row_to_json(v) FROM (SELECT poll_id FROM poll_votes WHERE poll_id = $1 AND user_id = $2) v',
                [pollId, ctx.actor_id]
            );
            if (already) {
                return { ok: false, error: 'You already voted on this poll' };
            }

            Ferum.db.query('INSERT INTO poll_votes (poll_id, option_id, user_id) VALUES ($1, $2, $3)', [pollId, optionId, ctx.actor_id]);
            Ferum.db.query('UPDATE poll_options SET votes = votes + 1 WHERE id = $1', [optionId]);

            var poll = Ferum.db.query(
                'SELECT row_to_json(p) FROM (SELECT created_by, question FROM polls WHERE id = $1) p',
                [pollId]
            );
            if (poll && poll.created_by && poll.created_by !== ctx.actor_id) {
                Ferum.forum.createNotification(
                    poll.created_by,
                    (payload.actor_username || 'Someone') + ' voted on your poll: "' + poll.question + '"'
                );
            }

            return { ok: true, data: { poll_id: pollId } };
        };

        __ferum_rpc['get_latest_poll'] = function () {
            var poll = Ferum.db.query(
                'SELECT row_to_json(p) FROM (SELECT id, question, creator_username, created_at FROM polls ORDER BY created_at DESC LIMIT 1) p'
            );
            if (!poll) {
                return { ok: true, data: null };
            }
            var options = Ferum.db.query(
                'SELECT jsonb_agg(o) FROM (SELECT id, label, votes FROM poll_options WHERE poll_id = $1 ORDER BY label) o',
                [poll.id]
            );
            poll.options = options || [];
            return { ok: true, data: poll };
        };

        Ferum.log.info('Community Polls RPC handlers registered');
    })();
}

// ─── Client-side: <ferum-slot-sidebar-left-top> widget ─────────────────────

if (typeof customElements !== 'undefined' && !customElements.get('ferum-slot-sidebar-left-top')) {
    (function () {
        var RPC_BASE = '/api/plugins/com.ferum.community-polls/rpc/';

        function call(action, body) {
            return fetch(RPC_BASE + action, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body || {}),
            }).then(function (r) { return r.json(); });
        }

        // The RPC endpoint's SUCCESS shape is { data: <value> } (DataResponse).
        // Its FAILURE shape is Ferum's standard API error, { error: { code,
        // message } } — NOT the { ok:false, error:"..." } contract a plugin's own
        // __ferum_rpc handler returns internally. That inner contract only decides
        // Ok/Err for the Rust Result inside script_runtime.rs; by the time it
        // reaches this fetch() call it's always the standard error envelope. Read
        // r.error.message here, not r.error directly, or a failed vote/create
        // renders the literal string "[object Object]" instead of the reason.
        function errorMessage(r) {
            if (!r || !r.error) return null;
            if (typeof r.error === 'string') return r.error;
            return r.error.message || 'Something went wrong.';
        }

        function escapeHtml(text) {
            var d = document.createElement('div');
            d.appendChild(document.createTextNode(text));
            return d.innerHTML;
        }

        var MAX_OPTIONS = 6;

        var STYLE = '' +
            '.cp-card{border:1px solid var(--bs-border-color,#495057);border-radius:.5rem;' +
            '  overflow:hidden;margin-bottom:1rem;background:var(--bs-card-bg,var(--bs-body-bg));}' +
            '.cp-header{display:flex;align-items:center;justify-content:space-between;gap:.5rem;' +
            '  padding:.6rem .75rem;background:var(--bs-tertiary-bg,rgba(255,255,255,.04));' +
            '  border-bottom:1px solid var(--bs-border-color,#495057);}' +
            '.cp-header-title{font-size:.8rem;font-weight:600;display:flex;align-items:center;gap:.4rem;}' +
            '.cp-body{padding:.75rem;}' +
            '.cp-new-btn{border:none;background:transparent;color:var(--bs-primary,#0d6efd);' +
            '  font-size:.72rem;font-weight:600;padding:.3rem .5rem;border-radius:.3rem;cursor:pointer;' +
            '  min-height:2rem;}' +
            '.cp-new-btn:hover{background:color-mix(in srgb, var(--bs-primary, #0d6efd) 12%, transparent);}' +
            '.cp-option{position:relative;overflow:hidden;border:1px solid var(--bs-border-color,#495057);' +
            '  border-radius:.4rem;margin-bottom:.4rem;cursor:pointer;transition:border-color .1s;' +
            '  min-height:2.75rem;display:flex;align-items:center;}' +
            '.cp-option:last-child{margin-bottom:0;}' +
            '.cp-option:hover{border-color:var(--bs-primary,#0d6efd);}' +
            '.cp-option-fill{position:absolute;inset:0;' +
            '  background:color-mix(in srgb, var(--bs-primary, #0d6efd) 16%, transparent);}' +
            '.cp-option-label{position:relative;display:flex;justify-content:space-between;align-items:center;' +
            '  gap:.5rem;padding:.5rem .65rem;font-size:.82rem;width:100%;}' +
            '.cp-option-pct{font-size:.72rem;color:var(--bs-secondary-color,#adb5bd);flex-shrink:0;}' +
            '.cp-meta{font-size:.72rem;color:var(--bs-secondary-color,#adb5bd);margin-top:.6rem;}' +
            '.cp-empty{padding:1rem .25rem;text-align:center;}';

        class CommunityPolls extends HTMLElement {
            connectedCallback() {
                this._mode = 'view'; // 'view' | 'create'
                this._formError = '';
                this._optionCount = 2;
                this.innerHTML =
                    '<style>' + STYLE + '</style>' +
                    '<div class="cp-card">' +
                    '  <div class="cp-header">' +
                    '    <span class="cp-header-title"><i class="fa-solid fa-square-poll-vertical"></i>Community Poll</span>' +
                    '    <button type="button" class="cp-new-btn" data-new><i class="fa-solid fa-plus me-1"></i>New</button>' +
                    '  </div>' +
                    '  <div class="cp-body" data-body></div>' +
                    '</div>';
                this._body = this.querySelector('[data-body]');
                this.querySelector('[data-new]').addEventListener('click', this._openCreateForm.bind(this));
                this._body.innerHTML = '<div class="small text-muted">Loading poll…</div>';
                this._load();
            }

            _load() {
                var self = this;
                call('get_latest_poll')
                    .then(function (r) { self._poll = (r && r.data) || null; self._paint(); })
                    .catch(function () {
                        self._body.innerHTML = '<div class="small text-danger">Failed to load poll.</div>';
                    });
            }

            _paint() {
                if (this._mode === 'create') {
                    this._renderCreateForm();
                } else {
                    this._renderPoll(this._poll);
                }
            }

            // ── Poll view ────────────────────────────────────────────────────────

            _renderPoll(poll) {
                var self = this;

                if (!poll) {
                    this._body.innerHTML =
                        '<div class="cp-empty text-muted small">' +
                        '<i class="fa-regular fa-face-smile fa-lg mb-2 d-block opacity-50"></i>' +
                        'No active poll yet. Start one!</div>';
                    return;
                }

                var total = poll.options.reduce(function (sum, o) { return sum + (o.votes || 0); }, 0);
                // Compact "poll bar" row instead of a stack of full-size buttons:
                // proportional fill in the background, label + percentage on top.
                // min-height:2.75rem keeps each row at/near the 44px tap-target guideline.
                var rows = poll.options.map(function (o) {
                    var pct = total > 0 ? Math.round((o.votes / total) * 100) : 0;
                    return '' +
                        '<div class="cp-option" data-vote="' + o.id + '" role="button" tabindex="0">' +
                        '  <div class="cp-option-fill" style="width:' + pct + '%;"></div>' +
                        '  <div class="cp-option-label">' +
                        '    <span>' + escapeHtml(o.label) + '</span>' +
                        '    <span class="cp-option-pct">' + pct + '%</span>' +
                        '  </div>' +
                        '</div>';
                }).join('');

                this._body.innerHTML =
                    '<div class="small fw-semibold mb-2">' + escapeHtml(poll.question) + '</div>' +
                    rows +
                    '<div class="small text-danger mt-2" data-vote-error style="display:none;"></div>' +
                    '<div class="cp-meta">' + total + ' vote' + (total === 1 ? '' : 's') + ' · started by ' + escapeHtml(poll.creator_username) + '</div>';

                this._body.querySelectorAll('[data-vote]').forEach(function (row) {
                    row.addEventListener('click', function () {
                        var errEl = self._body.querySelector('[data-vote-error]');
                        errEl.style.display = 'none';
                        call('vote', { poll_id: poll.id, option_id: row.getAttribute('data-vote') }).then(function (r) {
                            var msg = errorMessage(r);
                            if (msg) {
                                errEl.textContent = msg;
                                errEl.style.display = 'block';
                                return;
                            }
                            self._load();
                        });
                    });
                });
            }

            // ── Create-poll form (inline, replaces window.prompt/alert) ────────────

            _openCreateForm() {
                this._mode = 'create';
                this._formError = '';
                this._optionCount = 2;
                this._paint();
            }

            _renderCreateForm() {
                var self = this;
                var optionInputs = '';
                for (var i = 0; i < this._optionCount; i++) {
                    optionInputs += '<input type="text" class="form-control form-control-sm mb-2" ' +
                        'placeholder="Option ' + (i + 1) + '" data-opt maxlength="100">';
                }

                this._body.innerHTML =
                    '<form data-form>' +
                    '  <div class="small fw-semibold mb-2">New poll</div>' +
                    '  <input type="text" class="form-control form-control-sm mb-2" placeholder="Question" data-question maxlength="200">' +
                    '  <div data-options>' + optionInputs + '</div>' +
                    (this._optionCount < MAX_OPTIONS
                        ? '  <button type="button" class="btn btn-sm btn-link p-0 mb-2" data-add-option><i class="fa-solid fa-plus me-1"></i>Add option</button>'
                        : '') +
                    '  <div class="small text-danger mb-2" data-form-error style="display:' + (this._formError ? 'block' : 'none') + ';">' +
                    escapeHtml(this._formError) + '</div>' +
                    '  <div class="d-flex gap-2">' +
                    '    <button type="submit" class="btn btn-sm btn-primary flex-grow-1">Create poll</button>' +
                    '    <button type="button" class="btn btn-sm btn-outline-secondary" data-cancel>Cancel</button>' +
                    '  </div>' +
                    '</form>';

                var addBtn = this._body.querySelector('[data-add-option]');
                if (addBtn) {
                    addBtn.addEventListener('click', function () {
                        self._optionCount = Math.min(self._optionCount + 1, MAX_OPTIONS);
                        self._paint();
                    });
                }
                this._body.querySelector('[data-cancel]').addEventListener('click', function () {
                    self._mode = 'view';
                    self._paint();
                });
                this._body.querySelector('[data-form]').addEventListener('submit', function (e) {
                    e.preventDefault();
                    self._submitCreateForm();
                });
            }

            _submitCreateForm() {
                var self = this;
                var question = this._body.querySelector('[data-question]').value.trim();
                var options = Array.prototype.slice.call(this._body.querySelectorAll('[data-opt]'))
                    .map(function (input) { return input.value.trim(); })
                    .filter(function (v) { return v.length > 0; });

                if (!question) {
                    this._formError = 'Question is required.';
                    this._paint();
                    return;
                }
                if (options.length < 2) {
                    this._formError = 'Add at least 2 options.';
                    this._paint();
                    return;
                }

                call('create_poll', { question: question, options: options }).then(function (r) {
                    var msg = errorMessage(r);
                    if (msg) {
                        self._formError = msg;
                        self._paint();
                        return;
                    }
                    self._mode = 'view';
                    self._load();
                });
            }
        }

        customElements.define('ferum-slot-sidebar-left-top', CommunityPolls);
    })();
}
