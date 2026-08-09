/**
 * Ferum Board — shared frontend utilities
 * Loaded synchronously in both the public theme base and the admin base.
 * Exposes window.Ferum for use in page-specific inline scripts.
 */
(function (win) {
  'use strict';

  // ── Dates ──────────────────────────────────────────────────────────
  // The locale the server rendered this page in, from <html lang>. Every
  // client-side date goes through here rather than naming a locale inline: a
  // Vietnamese reader was getting "Feb 3, 2026" from JS-built markup next to
  // server-rendered dates in their own language, on the same screen.
  function pageLocale() {
    return document.documentElement.getAttribute('lang') || 'en';
  }

  // The viewer's timezone, as an IANA name, or null to mean "use this device's".
  //
  // Server-rendered timestamps are UTC — the server cannot know a guest's zone,
  // and baking a per-user zone into the HTML would make every page uncacheable
  // for the 70% of traffic that is not logged in. So conversion happens here.
  //
  // A signed-in user who has set a zone on their account overrides the device:
  // the server publishes it in <meta name="ferum-tz"> (a meta tag rather than an
  // inline script because the CSP forbids those). Without that tag — guests, and
  // users who never chose one — every helper below passes `undefined` to Intl,
  // which is exactly "use the device zone".
  //
  // Read once: it is server-rendered and cannot change without a reload.
  var _tz = null;
  var _tzRead = false;
  function tz() {
    if (!_tzRead) {
      var meta = document.querySelector('meta[name="ferum-tz"]');
      var v = meta && meta.getAttribute('content');
      _tz = v || null;
      _tzRead = true;
    }
    return _tz;
  }

  // Intl option sets, one per `localdate` style on the server. Keeping the names
  // aligned with the Tera filter's `style=` argument is what lets a template
  // declare its format once, in `data-style`, and have both renders agree.
  var _dateStyles = {
    date:      { year: 'numeric', month: 'short', day: 'numeric' },
    datetime:  { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' },
    daymonth:  { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' },
    monthyear: { year: 'numeric', month: 'short' },
  };

  /// Absolute timestamp in the viewer's zone and the page's language.
  ///
  /// Unlike the server's `localdate` filter — which localises month names via
  /// the Fluent catalog because chrono's `%b` is English-only — this leans on
  /// `Intl`, which already knows every locale's month names and field order.
  /// The two are not duplicates: they are the same intent on the two sides of a
  /// boundary where only one of them knows the zone.
  function formatAbs(dateStr, style) {
    var d = new Date(dateStr);
    if (isNaN(d.getTime())) return '';
    var opts = Object.assign({}, _dateStyles[style] || _dateStyles.date);
    var zone = tz();
    if (zone) opts.timeZone = zone;
    try {
      return d.toLocaleString(pageLocale(), opts);
    } catch (_) {
      // A bad IANA name from a stale profile must not blank out every date on
      // the page. Fall back to the device zone rather than throwing.
      delete opts.timeZone;
      return d.toLocaleString(pageLocale(), opts);
    }
  }

  /// Day-precision date, e.g. the timestamp on a freshly posted reply.
  function formatDate(dateStr) {
    return formatAbs(dateStr, 'date');
  }

  // ── Form input → wire format ───────────────────────────────────────
  // `<input type="datetime-local">` yields a ZONELESS wall-clock string
  // ("2026-08-15T09:00"). The API takes RFC 3339 with an offset and chrono
  // rejects anything else, so sending the raw value is not "assumed UTC" — it
  // is a 422 with the reason buried in a JSON body the form does not surface.
  //
  // That is exactly what the admin ban form did while the moderator ban form,
  // three files away, converted correctly. Both now call this, so the two
  // cannot drift apart again.
  //
  // `new Date(s)` on a zoneless string is interpreted in the BROWSER's zone,
  // which is the right reading: the admin typed a wall-clock time meaning their
  // own. `toISOString()` then converts that instant to UTC.
  //
  // Returns null for empty input — a ban with no end date is permanent, and the
  // API distinguishes that from a malformed one.
  function localInputToIso(value) {
    if (!value) return null;
    var d = new Date(value);
    // An unparseable value must not become the string "Invalid Date" on the
    // wire; let the caller treat it as absent and let validation speak.
    return isNaN(d.getTime()) ? null : d.toISOString();
  }

  // ── Timezone pickers ───────────────────────────────────────────────
  // Fills a <select> with every IANA zone the browser knows.
  //
  // Server-side rendering of ~400 <option>s would add tens of kilobytes to
  // every render of the page, for a list the browser already ships. So the
  // server renders only the options that must survive without JavaScript — the
  // current value, plus whatever neutral default the page wants — and this adds
  // the rest.
  //
  // Shared by /account and /admin/settings because they pick the same kind of
  // value; having two copies is how the two ban forms drifted apart.
  function fillTimezoneSelect(select) {
    if (!select || typeof Intl === 'undefined' || !Intl.supportedValuesOf) return;
    var zones;
    try {
      zones = Intl.supportedValuesOf('timeZone');
    } catch (_) {
      return; // Older browser: the server-rendered options still work.
    }
    // Skip anything already rendered, or the list shows duplicates and only one
    // of the pair carries the `selected` attribute.
    var existing = {};
    Array.prototype.forEach.call(select.options, function (o) { existing[o.value] = true; });

    var frag = document.createDocumentFragment();
    zones.forEach(function (z) {
      if (existing[z]) return;
      var opt = document.createElement('option');
      opt.value = z;
      opt.textContent = z;
      frag.appendChild(opt);
    });
    select.appendChild(frag);
  }

  // ── Relative timestamps ────────────────────────────────────────────
  // Through the catalog: these render on every list row and post header, and
  // were the largest block of hardcoded English left on the public pages.
  //
  // `Ferum.t` is defined lower in this file but only *called* at render time,
  // so the reference resolves. Past ~30 days this hands off to `formatDate`,
  // which localises through <html lang> rather than through a key.
  function timeAgo(dateStr) {
    var d = new Date(dateStr), s = Math.floor((Date.now() - d) / 1000);
    if (s < 60)      return Ferum.t('js-time-just-now');
    if (s < 3600)    return Ferum.t('js-time-minutes', { n: Math.floor(s / 60) });
    if (s < 86400)   return Ferum.t('js-time-hours',   { n: Math.floor(s / 3600) });
    if (s < 604800)  return Ferum.t('js-time-days',    { n: Math.floor(s / 86400) });
    if (s < 2592000) return Ferum.t('js-time-weeks',   { n: Math.floor(s / 604800) });
    return formatDate(dateStr);
  }

  // Rewrites every server-rendered timestamp into the viewer's zone.
  //
  // The server emits UTC — it has to, see `tz()` — so until this runs, every
  // date on the page is a UTC reading. That is the correct no-JavaScript
  // fallback (and the catalog labels the time-bearing formats "UTC" so it is
  // not silently wrong), but for everyone else it is off by the viewer's offset,
  // which for a reader at +07 means a post made at 06:30 local shows the
  // previous day's date.
  //
  // Two markers, because two kinds of timestamp want different treatment:
  //
  //   <time data-rel>              → "3d ago", with the exact local time in the
  //                                  tooltip. For anything whose recency is the
  //                                  point: post headers, list rows.
  //   <time data-abs data-style=…> → an absolute local date in the given style.
  //                                  For anything where "3 months ago" would be
  //                                  a regression — "Member since", a ban expiry,
  //                                  an audit trail.
  //
  // `data-abs` exists because the earlier code only handled `data-rel`, so a
  // template that wanted an absolute date had no marker to use and simply left
  // the UTC text in place.
  function initTimestamps() {
    document.querySelectorAll('time[data-rel], time[data-abs]').forEach(function (el) {
      var dt = el.getAttribute('datetime');
      if (!dt) return;
      var text = el.hasAttribute('data-abs')
        ? formatAbs(dt, el.getAttribute('data-style') || 'date')
        : timeAgo(dt);
      // An unparseable `datetime` yields '' from formatAbs, and timeAgo falls
      // through to it. Writing that would ERASE the server-rendered fallback and
      // leave a blank cell — strictly worse than showing a UTC time. Only
      // overwrite when we actually produced something.
      if (!text) return;
      // The tooltip carries the exact time; `toLocaleString()` with no locale
      // uses the *browser's*, which can differ from the language the page is
      // rendered in. Pinned to the page locale so the two agree.
      el.title = formatAbs(dt, 'datetime');
      el.textContent = text;
    });
  }

  // ── Theme toggle ───────────────────────────────────────────────────
  // Auto-discovers the button via [data-ferum-theme-btn] — works in both
  // the public nav and the admin topbar without hardcoding element IDs.
  function initThemeToggle() {
    var btn  = document.querySelector('[data-ferum-theme-btn]');
    if (!btn) return;
    var icon = btn.querySelector('i');

    function getTheme() { try { return localStorage.getItem('ferum-theme') || 'auto'; } catch { return 'auto'; } }

    function applyTheme(t) {
      if (t === 'auto') document.documentElement.removeAttribute('data-bs-theme');
      else              document.documentElement.setAttribute('data-bs-theme', t);
      if (icon) icon.className = t === 'dark' ? 'fa-solid fa-sun' : 'fa-solid fa-moon';
    }

    applyTheme(getTheme());
    btn.addEventListener('click', function () {
      var cur = getTheme();
      // Resolve 'auto' to the effective theme before toggling so the user
      // always gets the opposite of what they see, rather than landing on
      // a hardcoded value regardless of their system preference.
      var effectiveDark = cur === 'dark' ||
        (cur === 'auto' && window.matchMedia('(prefers-color-scheme: dark)').matches);
      var next = effectiveDark ? 'light' : 'dark';
      try { localStorage.setItem('ferum-theme', next); } catch {}
      applyTheme(next);

      // Logged-in users get this synced to their account (see base.html's
      // data-prefs-ssr) so it follows them to other devices/browsers, not
      // just this one. Fire-and-forget: the visual toggle above is instant
      // either way, this just makes the change durable.
      if (document.documentElement.hasAttribute('data-prefs-ssr') && win.FerumApi) {
        win.FerumApi.users.getPreferences()
          .then(function (r) { return r.ok ? r.json() : null; })
          .then(function (d) {
            var prefs = (d && d.data) || {};
            return win.FerumApi.users.updatePreferences(Object.assign({}, prefs, { theme: next }));
          })
          .catch(function () {});
      }
    });
  }

  // ── Shared confirmation modal ─────────────────────────────────────
  // Lazy-injects a single Bootstrap modal into <body> on first call.
  // Returns Promise<boolean> — true = confirmed, false = cancelled.
  // opts.typeToConfirm: string — if set, shows a type-to-confirm input and
  //   keeps the OK button disabled until the value matches exactly.
  var _confirmEl      = null;
  var _confirmResolve = null;

  // Escapes quotes as well as angle brackets. Every current call site happens to
  // interpolate into element content, where `& < >` alone would be enough — but
  // this is exported as a general-purpose helper, and the first time someone
  // writes data-x="<escaped>" a bare `"` would break out of the attribute and
  // turn any user-controlled name into stored XSS. Escaping all five is the only
  // version that is correct in both contexts.
  var HTML_ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
  function escapeHtml(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) {
      return HTML_ESCAPES[c];
    });
  }

  // ── API error text ─────────────────────────────────────────────────
  // Every JSON endpoint answers a failure with the envelope documented in
  // CLAUDE.md: { error: { code, message } }, where `message` has already been
  // translated into the reader's locale by the translate_errors middleware.
  //
  // Pulling that out was written by hand at 49 call sites across 11 files as
  // `(body.error && body.error.message) || fallback`.
  //
  // The `|| fallback` deliberately stays at the call site rather than becoming
  // a second argument: the wording belongs next to the code that chose it, and
  // several callers pass a `Ferum.t(...)` lookup that reads better inline.
  //
  // Returning '' rather than undefined for a missing message is what keeps
  // `|| fallback` firing on an envelope whose message is an empty string —
  // matching what the hand-written expression did.
  //
  // Takes a parsed body, not a Response: callers reach this point having
  // already read the body, usually as
  // `await res.json().catch(function () { return {}; })`.

  function errorMessage(body) {
    return (body && body.error && body.error.message) || '';
  }

  // ── Reference-list <select> ────────────────────────────────────────
  // Appends one <option> per { id, name } to a select, leaving whatever is
  // already there (a placeholder row, the server-rendered current value).
  //
  // Written out identically in the two forms that pick a brand — the product
  // detail dialog and the new-thread composer. Built with createElement rather
  // than an innerHTML string so a brand name never has to be escaped: setting
  // textContent cannot introduce markup, which is the failure mode a string
  // template invites.
  function fillSelect(select, items) {
    if (!select) return;
    var frag = document.createDocumentFragment();
    (items || []).forEach(function (it) {
      var o = document.createElement('option');
      o.value = it.id;
      o.textContent = it.name;
      frag.appendChild(o);
    });
    select.appendChild(frag);
  }

  // ── Number grouping ────────────────────────────────────────────────
  // The client-side counterpart of the `thousands` Tera filter, and a
  // deliberate mirror of it: a price the browser draws sits next to prices the
  // server rendered, so the two must group digits identically.
  //
  // Three files each built their own `new Intl.NumberFormat('vi-VN')`, which
  // pinned every JS-rendered price to Vietnamese grouping no matter what
  // language the page was in — while the server followed the reader's catalog.
  // The separator comes from the same catalog key both sides read; Intl is not
  // used because its locale is the browser's, not the page's.
  //
  // Same fallback rule as the filter: a catalog that omits the key resolves to
  // the key name, so anything but a single character falls back to a comma.
  function formatNumber(n) {
    var num = typeof n === 'number' ? n : parseInt(String(n).replace(/[^\d-]/g, ''), 10);
    if (!isFinite(num)) return '';
    var sep = Ferum.t('js-thousands-separator');
    if (!sep || sep.length !== 1) sep = ',';
    var digits = String(Math.abs(num));
    var out = '';
    for (var i = 0; i < digits.length; i++) {
      if (i > 0 && (digits.length - i) % 3 === 0) out += sep;
      out += digits[i];
    }
    return num < 0 ? '-' + out : out;
  }

  function ensureConfirmModal() {
    if (_confirmEl) return;
    var wrapper = document.createElement('div');
    wrapper.innerHTML =
      '<div id="fr-confirm-modal" class="modal fade" tabindex="-1" aria-hidden="true">' +
        '<div class="modal-dialog modal-dialog-centered modal-sm">' +
          '<div class="modal-content">' +
            '<div class="modal-header border-0 pb-1">' +
              '<h5 class="modal-title fs-6 fw-semibold" id="fr-confirm-title"></h5>' +
              '<button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>' +
            '</div>' +
            '<div class="modal-body pt-1 small text-muted" id="fr-confirm-body"></div>' +
            '<div class="modal-body pt-0 pb-2 d-none" id="fr-confirm-type-wrap">' +
              '<label class="form-label small fw-semibold mb-1" id="fr-confirm-type-label"></label>' +
              '<input type="text" class="form-control form-control-sm font-monospace" id="fr-confirm-type-input" autocomplete="off">' +
            '</div>' +
            '<div class="modal-footer border-0 pt-1 gap-2">' +
              '<button type="button" class="btn btn-secondary btn-sm" data-bs-dismiss="modal">Cancel</button>' +
              '<button type="button" class="btn btn-sm" id="fr-confirm-ok">Confirm</button>' +
            '</div>' +
          '</div>' +
        '</div>' +
      '</div>';
    _confirmEl = wrapper.firstChild;
    document.body.appendChild(_confirmEl);

    document.getElementById('fr-confirm-ok').addEventListener('click', function () {
      win.bootstrap.Modal.getInstance(_confirmEl)?.hide();
      if (_confirmResolve) { _confirmResolve(true); _confirmResolve = null; }
    });
    _confirmEl.addEventListener('hidden.bs.modal', function () {
      if (_confirmResolve) { _confirmResolve(false); _confirmResolve = null; }
    });
  }

  function showConfirm(title, body, okLabel, okVariant, opts) {
    okLabel   = okLabel   || Ferum.t('js-confirm');
    okVariant = okVariant || 'danger';
    opts      = opts      || {};

    ensureConfirmModal();

    document.getElementById('fr-confirm-title').textContent = title;
    document.getElementById('fr-confirm-body').textContent  = body;

    var okBtn = document.getElementById('fr-confirm-ok');
    okBtn.textContent = okLabel;
    okBtn.className   = 'btn btn-' + okVariant + ' btn-sm';

    var typeWrap  = document.getElementById('fr-confirm-type-wrap');
    var typeLabel = document.getElementById('fr-confirm-type-label');
    var typeInput = document.getElementById('fr-confirm-type-input');

    if (opts.typeToConfirm) {
      typeLabel.innerHTML  = Ferum.t('js-type-to-confirm', { code: '<code class="user-select-all">' + escapeHtml(opts.typeToConfirm) + '</code>' });
      typeInput.value      = '';
      typeInput.placeholder = opts.typeToConfirm;
      typeWrap.classList.remove('d-none');
      okBtn.disabled  = true;
      typeInput.oninput = function () {
        okBtn.disabled = (typeInput.value !== opts.typeToConfirm);
      };
      _confirmEl.addEventListener('shown.bs.modal', function focusInput() {
        typeInput.focus();
        _confirmEl.removeEventListener('shown.bs.modal', focusInput);
      });
    } else {
      typeWrap.classList.add('d-none');
      typeInput.oninput = null;
      okBtn.disabled    = false;
    }

    return new Promise(function (resolve) {
      _confirmResolve = resolve;
      win.bootstrap.Modal.getOrCreateInstance(_confirmEl).show();
    });
  }

  // ── Fire-and-forget toast ─────────────────────────────────────────
  // Works on any page without a pre-existing toast element.
  // Lazily creates #fr-toast-container on first call.
  // isError: true → danger (6 s), false/omit → success (3.5 s).
  // Bootstrap variant → FontAwesome icon. Shared by toast() and showFeedback()
  // below: the same severity must look the same whether it surfaces as a toast
  // or as an inline alert, and two copies of this table is how that stops being
  // true without anyone noticing.
  var _variantIconMap = { success: 'fa-check', danger: 'fa-circle-exclamation', warning: 'fa-triangle-exclamation', info: 'fa-circle-info' };

  function toast(msg, isError) {
    var variant   = isError ? 'danger' : 'success';
    var delay     = isError ? 6000 : 3500;
    var container = document.getElementById('fr-toast-container');
    if (!container) {
      container = document.createElement('div');
      container.id        = 'fr-toast-container';
      container.className = 'toast-container position-fixed bottom-0 end-0 p-3';
      container.style.zIndex = '9999';
      document.body.appendChild(container);
    }
    var el = document.createElement('div');
    el.className = 'toast align-items-center border-0 text-bg-' + variant;
    el.setAttribute('role', isError ? 'alert' : 'status');
    el.setAttribute('aria-live', isError ? 'assertive' : 'polite');
    el.setAttribute('aria-atomic', 'true');
    var wrap = document.createElement('div'); wrap.className = 'd-flex';
    var body = document.createElement('div'); body.className = 'toast-body d-flex align-items-center gap-2';
    var icon = document.createElement('i');
    icon.className = 'fa-solid ' + (_variantIconMap[variant] || 'fa-circle-info') + ' flex-shrink-0';
    icon.setAttribute('aria-hidden', 'true');
    body.appendChild(icon);
    body.appendChild(document.createTextNode(msg));
    var closeBtn = document.createElement('button');
    closeBtn.type      = 'button';
    closeBtn.className = 'btn-close btn-close-white me-2 m-auto';
    closeBtn.setAttribute('data-bs-dismiss', 'toast');
    closeBtn.setAttribute('aria-label', 'Close');
    wrap.appendChild(body); wrap.appendChild(closeBtn); el.appendChild(wrap);
    container.appendChild(el);
    new win.bootstrap.Toast(el, { delay: delay }).show();
    el.addEventListener('hidden.bs.toast', function () { el.remove(); });
  }

  // ── Named-element toast ────────────────────────────────────────────
  // elementId: id of the .toast element (must contain a .toast-body child).
  // Kept for mod templates that have pre-existing toast containers.
  function showToast(elementId, msg, isError) {
    var el    = document.getElementById(elementId);
    var msgEl = el && el.querySelector('.toast-body');
    if (!el || !msgEl) return;
    el.className = 'toast align-items-center position-fixed bottom-0 end-0 m-3 border-0 text-bg-' +
                   (isError ? 'danger' : 'success');
    msgEl.textContent = msg;
    new win.bootstrap.Toast(el, { delay: isError ? 6000 : 3500 }).show();
  }

  // ── Inline feedback alert ──────────────────────────────────────────
  // Renders a Bootstrap alert in-place with a type-appropriate icon.
  // Errors (danger) stay visible until the next call; all other types
  // auto-dismiss after 3.5 s so they don't clutter the page.
  function showFeedback(elementId, type, msg) {
    var el = document.getElementById(elementId);
    if (!el) return;
    el.className = 'alert alert-' + type + ' py-1 small d-flex align-items-center gap-1';
    el.innerHTML = '';
    var iconKey = _variantIconMap[type];
    if (iconKey) {
      var i = document.createElement('i');
      i.className = 'fa-solid ' + iconKey + ' flex-shrink-0';
      i.setAttribute('aria-hidden', 'true');
      el.appendChild(i);
    }
    el.appendChild(document.createTextNode(msg));
    el.classList.remove('d-none');
    if (type !== 'danger') {
      setTimeout(function () { el.classList.add('d-none'); }, 3500);
    }
  }

  // ── Password visibility toggle ─────────────────────────────────────
  // Finds the .fr-password-toggle button that is a sibling of the input
  // inside .fr-password-wrapper, then wires the eye-icon click.
  function initPasswordToggle(inputId) {
    var input = document.getElementById(inputId);
    if (!input) return;
    var btn  = input.parentElement && input.parentElement.querySelector('.fr-password-toggle');
    var icon = btn && btn.querySelector('i');
    if (!btn || !icon) return;
    btn.addEventListener('click', function () {
      var show = input.type === 'password';
      input.type      = show ? 'text'     : 'password';
      icon.className  = show ? 'fa-regular fa-eye-slash' : 'fa-regular fa-eye';
    });
  }

  // ── Active nav link highlighting ───────────────────────────────────
  function initNavActive() {
    var path = window.location.pathname;
    document.querySelectorAll('.fr-nav-link, .fr-bn-item').forEach(function (el) {
      var href = el.getAttribute('href');
      if (!href) return;
      var isActive = href === '/'
        ? path === '/'
        : path === href || path.startsWith(href + '/');
      if (isActive) el.classList.add('active');
    });
  }

  // ── Mobile sidebar drawer ─────────────────────────────────────────
  function initMobileSidebar() {
    function toggleSidebar() {
      var overlay = document.getElementById('mobileSidebar');
      var toggleBtn = document.getElementById('sidebarToggleBtn');
      if (!overlay) return;
      var isOpen = overlay.classList.contains('is-open');
      if (isOpen) {
        overlay.classList.remove('is-open');
        if (toggleBtn) toggleBtn.setAttribute('aria-expanded', 'false');
        setTimeout(function () { overlay.style.display = 'none'; }, 270);
      } else {
        overlay.style.display = 'flex';
        requestAnimationFrame(function () {
          requestAnimationFrame(function () {
            overlay.classList.add('is-open');
            if (toggleBtn) toggleBtn.setAttribute('aria-expanded', 'true');
          });
        });
      }
    }
    document.querySelectorAll('[data-action="toggle-sidebar"]').forEach(function (el) {
      el.addEventListener('click', toggleSidebar);
    });

    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') {
        var overlay = document.getElementById('mobileSidebar');
        if (overlay && overlay.classList.contains('is-open')) toggleSidebar();
      }
    });
  }

  // ── Mobile search toggle ──────────────────────────────────────────
  // The topbar search is inline on desktop and collapsed to an icon button on
  // mobile. Tapping the button drops the search bar from under the sticky header
  // (CSS keys off `.search-open` on `.fr-topbar`). Closes on Escape, on submit,
  // and on any tap outside the bar. No-ops on desktop where the button is hidden.
  function initSearchToggle() {
    var topbar = document.querySelector('.fr-topbar');
    var toggle = document.querySelector('[data-action="toggle-search"]');
    if (!topbar || !toggle) return;
    var form = document.getElementById('topbarSearch');
    var input = form ? form.querySelector('input[type="search"]') : null;
    var icon = toggle.querySelector('i');

    function setOpen(open) {
      topbar.classList.toggle('search-open', open);
      toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
      if (icon) {
        icon.classList.toggle('fa-magnifying-glass', !open);
        icon.classList.toggle('fa-xmark', open);
      }
      if (open && input) input.focus();
    }

    toggle.addEventListener('click', function (e) {
      e.stopPropagation();
      setOpen(!topbar.classList.contains('search-open'));
    });
    // A tap inside the bar must not close it; only an outside tap does.
    if (form) form.addEventListener('click', function (e) { e.stopPropagation(); });
    document.addEventListener('click', function () {
      if (topbar.classList.contains('search-open')) setOpen(false);
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && topbar.classList.contains('search-open')) {
        setOpen(false);
        toggle.focus();
      }
    });
  }

  // ── Login return URL ──────────────────────────────────────────────
  // Injects ?next=<current-path> into every a[href="/login"] so the
  // login page can redirect back after a successful sign-in.
  // Skipped on auth pages themselves to prevent redirect loops.
  function initLoginReturnUrl() {
    var path = window.location.pathname;
    if (path === '/login' || path === '/register' ||
        path === '/forgot-password' || path.startsWith('/reset-password')) return;
    var next = path + window.location.search;
    document.querySelectorAll('a[href="/login"]').forEach(function (el) {
      el.href = '/login?next=' + encodeURIComponent(next);
    });
  }

  // ── Bootstrap tooltips ─────────────────────────────────────────────
  function initTooltips() {
    if (!win.bootstrap || !win.bootstrap.Tooltip) return;
    document.querySelectorAll('[data-bs-toggle="tooltip"]').forEach(function (el) {
      new win.bootstrap.Tooltip(el);
    });
  }

  // ── Logout button ─────────────────────────────────────────────────
  function initLogout() {
    document.querySelectorAll('[data-action="logout"]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        if (typeof FerumApi !== 'undefined') {
          FerumApi.auth.logout().then(function () { location.href = '/'; });
        }
      });
    });
  }

  // ── Password strength meter ───────────────────────────────────────
  // Wires an input to drive a visual strength bar + label.
  // fillId: inner div whose width + background-color we animate.
  // textId: element that receives the label ("Weak", "Strong", …).
  function initPasswordStrength(inputId, fillId, textId) {
    var input = document.getElementById(inputId);
    var fill  = document.getElementById(fillId);
    var text  = document.getElementById(textId);
    if (!input || !fill || !text) return;
    input.addEventListener('input', function () {
      var val = this.value;
      if (!val) {
        fill.style.width = '0'; fill.style.background = '';
        text.textContent = ''; text.style.color = '';
        return;
      }
      var score = 0;
      if (val.length >= 8)  score++;
      if (val.length >= 12) score++;
      if (/[A-Z]/.test(val) && /[a-z]/.test(val)) score++;
      if (/[0-9]/.test(val)) score++;
      if (/[^A-Za-z0-9]/.test(val)) score++;
      var levels = [
        { pct: '20%', color: '#ef4444', label: Ferum.t('js-pw-very-weak')   },
        { pct: '40%', color: '#f97316', label: Ferum.t('js-pw-weak')        },
        { pct: '60%', color: '#eab308', label: Ferum.t('js-pw-fair')        },
        { pct: '80%', color: '#22c55e', label: Ferum.t('js-pw-strong')      },
        { pct: '100%',color: '#10b981', label: Ferum.t('js-pw-very-strong') },
      ];
      var lvl = levels[Math.min(score, levels.length) - 1] || levels[0];
      fill.style.width      = lvl.pct;
      fill.style.background = lvl.color;
      text.textContent      = lvl.label;
      text.style.color      = lvl.color;
    });
  }

  // ── Auto-init on DOMContentLoaded ─────────────────────────────────
  function onReady(fn) {
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', fn);
    } else {
      fn();
    }
  }
  onReady(initTimestamps);
  onReady(initThemeToggle);
  onReady(initNavActive);
  onReady(initMobileSidebar);
  onReady(initSearchToggle);
  onReady(initLogout);
  onReady(initLoginReturnUrl);
  onReady(initTooltips);

  win.Ferum = {
    toast:                toast,
    showToast:            showToast,
    showFeedback:         showFeedback,
    showConfirm:          showConfirm,
    initPasswordToggle:   initPasswordToggle,
    initPasswordStrength: initPasswordStrength,
    escapeHtml:           escapeHtml,
    errorMessage:         errorMessage,
    fillSelect:           fillSelect,
    formatNumber:         formatNumber,
    formatDate:           formatDate,
    formatAbs:            formatAbs,
    timeAgo:              timeAgo,
    localInputToIso:      localInputToIso,
    fillTimezoneSelect:   fillTimezoneSelect,
    pageLocale:           pageLocale,
    tz:                   tz,
    // Exposed so content injected after load (Alpine lists, fetch-rendered
    // rows) can localise its own <time> elements instead of leaving them UTC.
    initTimestamps:       initTimestamps,
  };
}(window));

// ── Sidebar nav active state (replaces inline script in sidebar_left.html) ──
(function () {
  var nav = document.getElementById('sidebar-nav');
  if (!nav) return;
  var path = window.location.pathname;
  nav.querySelectorAll('a[data-nav-exact], a[data-nav-prefix]').forEach(function (a) {
    var exact  = a.getAttribute('data-nav-exact');
    var prefix = a.getAttribute('data-nav-prefix');
    var href   = a.getAttribute('href');
    var active = exact ? path === href : (prefix ? path.startsWith(prefix) : false);
    if (active) a.classList.add('active');
  });
}());

// ── Language switcher ───────────────────────────────────────────────────────
// Posts the choice to the server rather than storing it client-side. Unlike the
// theme toggle (which localStorage can apply after paint), the language is baked
// into the server-rendered HTML, so switching necessarily means a round trip and
// a reload. The server sets the `ferum_locale` cookie; for signed-in users it
// also persists the choice to their account.
(function () {
  var buttons = document.querySelectorAll('[data-ferum-locale]');
  if (!buttons.length) return;

  buttons.forEach(function (btn) {
    btn.addEventListener('click', function () {
      var locale = btn.getAttribute('data-ferum-locale');
      btn.disabled = true;

      fetch('/api/locale', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ locale: locale }),
      })
        .then(function (res) {
          if (!res.ok) throw new Error('locale switch failed: ' + res.status);
          // Full reload rather than a re-render: every string on the page came
          // from the server in the old language.
          window.location.reload();
        })
        .catch(function (err) {
          btn.disabled = false;
          if (window.Ferum && window.Ferum.toast) {
            window.Ferum.toast(Ferum.t('js-could-not-change-language'), true);
          } else {
            console.error(err);
          }
        });
    });
  });
}());

// ── Client-side translation lookup ──────────────────────────────────────────
// The server puts a `js-*`-scoped dictionary into <meta name="ferum-i18n">
// (a data attribute rather than an inline <script>, because the CSP forbids
// inline scripts). Everything the browser renders itself goes through this.
//
// Falls back to the key so a missing string is visible and greppable rather
// than rendering as an empty label.
(function (w) {
  var dict = {};
  try {
    var meta = document.querySelector('meta[name="ferum-i18n"]');
    if (meta) dict = JSON.parse(meta.getAttribute('content') || '{}');
  } catch (_) {
    // A malformed dictionary must not stop the page's scripts from running.
  }

  w.Ferum = w.Ferum || {};
  w.Ferum.i18n = dict;

  /**
   * t('js-network-error') -> Ferum.t('js-network-error')
   * Optional `args` interpolate { $name } placeables client-side.
   */
  w.Ferum.t = function (key, args) {
    var text = dict[key];
    if (text === undefined) return key;
    if (!args) return text;
    return text.replace(/\{\s*\$(\w+)\s*\}/g, function (m, name) {
      return Object.prototype.hasOwnProperty.call(args, name) ? args[name] : m;
    });
  };
}(window));
