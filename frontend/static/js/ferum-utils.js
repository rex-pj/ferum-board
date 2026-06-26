/**
 * Ferum Board — shared frontend utilities
 * Loaded synchronously in both the public theme base and the admin base.
 * Exposes window.Ferum for use in page-specific inline scripts.
 */
(function (win) {
  'use strict';

  // ── Relative timestamps ────────────────────────────────────────────
  function timeAgo(dateStr) {
    var d = new Date(dateStr), s = Math.floor((Date.now() - d) / 1000);
    if (s < 60)      return 'just now';
    if (s < 3600)    return Math.floor(s / 60) + 'm ago';
    if (s < 86400)   return Math.floor(s / 3600) + 'h ago';
    if (s < 604800)  return Math.floor(s / 86400) + 'd ago';
    if (s < 2592000) return Math.floor(s / 604800) + 'w ago';
    return d.toLocaleDateString('en', { month: 'short', day: 'numeric', year: 'numeric' });
  }

  function initRelativeTimes() {
    document.querySelectorAll('time[data-rel]').forEach(function (el) {
      var dt = el.getAttribute('datetime');
      if (dt) { el.title = new Date(dt).toLocaleString(); el.textContent = timeAgo(dt); }
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
    });
  }

  // ── Shared confirmation modal ─────────────────────────────────────
  // Lazy-injects a single Bootstrap modal into <body> on first call.
  // Returns Promise<boolean> — true = confirmed, false = cancelled.
  // opts.typeToConfirm: string — if set, shows a type-to-confirm input and
  //   keeps the OK button disabled until the value matches exactly.
  var _confirmEl      = null;
  var _confirmResolve = null;

  function escapeHtml(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
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
    okLabel   = okLabel   || 'Confirm';
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
      typeLabel.innerHTML  = 'Type <code class="user-select-all">' + escapeHtml(opts.typeToConfirm) + '</code> to confirm:';
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
  var _toastIconMap = { success: 'fa-check', danger: 'fa-circle-exclamation', warning: 'fa-triangle-exclamation', info: 'fa-circle-info' };

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
    icon.className = 'fa-solid ' + (_toastIconMap[variant] || 'fa-circle-info') + ' flex-shrink-0';
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
  var _feedbackIconMap = { success: 'fa-check', danger: 'fa-circle-exclamation', warning: 'fa-triangle-exclamation', info: 'fa-circle-info' };

  function showFeedback(elementId, type, msg) {
    var el = document.getElementById(elementId);
    if (!el) return;
    el.className = 'alert alert-' + type + ' py-1 small d-flex align-items-center gap-1';
    el.innerHTML = '';
    var iconKey = _feedbackIconMap[type];
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
    win.toggleMobileSidebar = toggleSidebar;

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
        { pct: '20%', color: '#ef4444', label: 'Very weak'   },
        { pct: '40%', color: '#f97316', label: 'Weak'        },
        { pct: '60%', color: '#eab308', label: 'Fair'        },
        { pct: '80%', color: '#22c55e', label: 'Strong'      },
        { pct: '100%',color: '#10b981', label: 'Very strong' },
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
  onReady(initRelativeTimes);
  onReady(initThemeToggle);
  onReady(initNavActive);
  onReady(initMobileSidebar);
  onReady(initLogout);

  win.Ferum = {
    toast:                toast,
    showToast:            showToast,
    showFeedback:         showFeedback,
    showConfirm:          showConfirm,
    initPasswordToggle:   initPasswordToggle,
    initPasswordStrength: initPasswordStrength,
    escapeHtml:           escapeHtml,
  };
}(window));
