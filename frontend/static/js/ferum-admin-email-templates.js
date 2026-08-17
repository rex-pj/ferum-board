// /admin/email-templates — edit, preview and test transactional email copy.
//
// Two things here are load-bearing rather than cosmetic:
//
// * The preview goes into a `sandbox`ed iframe with NO allow-same-origin. The
//   document is admin-authored HTML, so rendering it with this origin's
//   privileges would make the editor a self-XSS surface — and `srcdoc` inherits
//   the embedding origin unless the sandbox withholds it.
// * Preview and test-send POST the *draft*, not the saved row, so what is shown
//   is what the current textarea would send.
(function () {
  'use strict';

  var list = document.getElementById('tpl-list');
  if (!list) return;

  var localeSelect = document.getElementById('tpl-locale');
  var editor = document.getElementById('tpl-editor');
  var previewCard = document.getElementById('tpl-preview-card');

  var templates = [];
  var current = null;

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function status(cls, text) {
    var box = document.getElementById('tpl-status');
    box.className = 'mt-2 small ' + (cls || '');
    box.textContent = text;
  }

  async function request(method, url, body) {
    var opts = { method: method, headers: { Accept: 'application/json' } };
    if (body !== undefined) {
      opts.headers['Content-Type'] = 'application/json';
      opts.body = JSON.stringify(body);
    }
    var res = await fetch(url, opts);
    var payload = null;
    try {
      payload = await res.json();
    } catch (e) {
      payload = null;
    }
    if (!res.ok) {
      // The server's message is the useful half — a validation refusal names
      // the variable that is wrong, which "HTTP 422" does not.
      var msg = payload && payload.error && payload.error.message;
      throw new Error(msg || 'HTTP ' + res.status);
    }
    return payload ? payload.data : null;
  }

  function draft() {
    return {
      subject: document.getElementById('tpl-subject').value,
      body_html: document.getElementById('tpl-body').value
    };
  }

  function base() {
    return '/api/admin/email-templates/' + encodeURIComponent(current.key) +
      '/' + encodeURIComponent(localeSelect.value);
  }

  // ── List ──────────────────────────────────────────────────────────────────

  function renderList() {
    list.innerHTML = '';
    templates.forEach(function (t) {
      var item = el('button', 'list-group-item list-group-item-action');
      item.type = 'button';
      if (current && current.key === t.key) item.classList.add('active');

      item.appendChild(el('div', 'fw-semibold', t.key));
      var edited = t.customised_locales.indexOf(localeSelect.value) !== -1;
      item.appendChild(el(
        'div',
        'small ' + (current && current.key === t.key ? '' : 'text-body-secondary'),
        edited ? '●' : ''
      ));
      item.onclick = function () { select(t); };
      list.appendChild(item);
    });
  }

  function renderVariables(vars) {
    var box = document.getElementById('tpl-variables');
    box.innerHTML = '';
    vars.forEach(function (v) {
      var cls = v.kind === 'url' ? 'text-bg-info'
        : v.kind === 'raw' ? 'text-bg-warning' : 'text-bg-secondary';
      var badge = el('span', 'badge ' + cls, '{{ ' + v.name + ' }}');
      badge.title = v.kind + (v.required ? ' — required in the body' : '');
      box.appendChild(badge);
    });
  }

  async function select(t) {
    current = t;
    renderList();
    editor.classList.remove('d-none');
    previewCard.classList.add('d-none');
    document.getElementById('tpl-key').textContent = t.key;
    document.getElementById('tpl-description').textContent = t.description;
    renderVariables(t.variables);
    status('text-muted', '');

    try {
      var data = await request('GET', base());
      document.getElementById('tpl-subject').value = data.subject;
      document.getElementById('tpl-body').value = data.body_html;
      document.getElementById('tpl-customised').classList.toggle('d-none', !data.customised);
      document.getElementById('tpl-default').classList.toggle('d-none', data.customised);
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  // ── Actions ───────────────────────────────────────────────────────────────

  async function save() {
    status('text-muted', '…');
    try {
      await request('PUT', base(), draft());
      status('text-success', document.getElementById('tpl-save-btn').dataset.savedText || 'Saved.');
      await reload();
      await select(current);
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  async function preview() {
    status('text-muted', '…');
    try {
      var data = await request('POST', base() + '/preview', draft());
      document.getElementById('tpl-preview-subject').textContent = data.subject;
      document.getElementById('tpl-preview-text').textContent = data.text;
      // `srcdoc`, not a data: URL — the CSP forbids the latter as a frame
      // source, and the sandbox attribute already withholds the origin.
      document.getElementById('tpl-preview-frame').srcdoc = data.html;
      previewCard.classList.remove('d-none');
      status('text-muted', '');
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  async function testSend() {
    status('text-muted', '…');
    try {
      var data = await request('POST', base() + '/test', draft());
      if (data.success) {
        status('text-success', 'Sent to ' + data.sent_to + '.');
      } else {
        status('text-danger', data.error || 'Send failed.');
      }
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  async function reset() {
    var confirmText = document.getElementById('tpl-reset-btn').dataset.confirmText ||
      'Discard your edits for this language?';
    if (!window.confirm(confirmText)) return;
    status('text-muted', '…');
    try {
      await request('DELETE', base());
      await reload();
      await select(current);
      status('text-success',
        document.getElementById('tpl-reset-btn').dataset.resetText || 'Reset to the default copy.');
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  async function reload() {
    templates = await request('GET', '/api/admin/email-templates');
    renderList();
  }

  document.getElementById('tpl-save-btn').onclick = save;
  document.getElementById('tpl-preview-btn').onclick = preview;
  document.getElementById('tpl-test-btn').onclick = testSend;
  document.getElementById('tpl-reset-btn').onclick = reset;
  localeSelect.onchange = function () {
    renderList();
    if (current) select(current);
  };

  reload().catch(function (e) {
    list.innerHTML = '';
    list.appendChild(el('div', 'list-group-item text-danger small', e.message));
  });
})();
