// /admin/email-templates — edit, preview and test transactional email copy.
//
// Three things here are load-bearing rather than cosmetic:
//
// * The preview goes into a `sandbox`ed iframe with NO allow-same-origin. The
//   document is admin-authored HTML, so rendering it with this origin's
//   privileges would make the editor a self-XSS surface — `srcdoc` inherits the
//   embedding origin unless the sandbox withholds it.
// * Preview and test-send POST the *draft*, not the saved row, so what is shown
//   is what the current textarea would send.
// * Switching template or language replaces the textarea contents, so both are
//   guarded on a dirty check. Without it the edit is gone with no warning and
//   nothing to undo from.
(function () {
  'use strict';

  var list = document.getElementById('tpl-list');
  if (!list) return;

  var sharedList = document.getElementById('tpl-list-shared');
  var sharedLabel = document.getElementById('tpl-shared-label');
  var localeSelect = document.getElementById('tpl-locale');
  var editor = document.getElementById('tpl-editor');
  var previewCard = document.getElementById('tpl-preview-card');
  var subjectInput = document.getElementById('tpl-subject');
  var bodyInput = document.getElementById('tpl-body');
  var dirtyBadge = document.getElementById('tpl-dirty');

  var templates = [];
  var current = null;
  // What was last loaded or saved. Compared against the inputs rather than
  // tracking a boolean, so typing and undoing leaves no false "unsaved".
  var baseline = { subject: '', body_html: '' };

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function status(cls, text) {
    var box = document.getElementById('tpl-status');
    box.className = 'small ms-auto ' + (cls || '');
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
    return { subject: subjectInput.value, body_html: bodyInput.value };
  }

  function isDirty() {
    return subjectInput.value !== baseline.subject ||
      bodyInput.value !== baseline.body_html;
  }

  function markClean() {
    baseline = draft();
    dirtyBadge.classList.add('d-none');
  }

  /// Returns false when the user chose to keep editing.
  function mayLeave() {
    if (!isDirty()) return true;
    var msg = list.dataset.discardText ||
      'You have unsaved changes to this template. Discard them?';
    return window.confirm(msg);
  }

  function base() {
    return '/api/admin/email-templates/' + encodeURIComponent(current.key) +
      '/' + encodeURIComponent(localeSelect.value);
  }

  // ── List ──────────────────────────────────────────────────────────────────

  function renderList() {
    list.innerHTML = '';
    sharedList.innerHTML = '';

    var anyShared = false;
    templates.forEach(function (t) {
      var target = t.is_layout ? sharedList : list;
      if (t.is_layout) anyShared = true;

      var item = el('button', 'list-group-item list-group-item-action py-2');
      item.type = 'button';
      if (current && current.key === t.key) item.classList.add('active');

      var row = el('div', 'd-flex align-items-center gap-2');
      row.appendChild(el('span', 'me-auto', t.name));
      // Only the edited state gets a marker. A dot on every row — which is what
      // pre-seeding the table produced — distinguishes nothing.
      if (t.customised_locales.indexOf(localeSelect.value) !== -1) {
        var label = list.dataset.editedText || 'Edited';
        var mark = el('span', 'small', '●');
        // The dot is the only carrier of this state, so it needs a name for
        // anyone not reading the colour.
        mark.setAttribute('aria-label', label);
        mark.title = label;
        row.appendChild(mark);
      }
      item.appendChild(row);
      item.onclick = function () {
        if (current && current.key === t.key) return;
        if (!mayLeave()) return;
        select(t);
      };
      target.appendChild(item);
    });

    sharedLabel.classList.toggle('d-none', !anyShared);
    sharedList.classList.toggle('d-none', !anyShared);
  }

  function insertAtCursor(text) {
    var start = bodyInput.selectionStart;
    var end = bodyInput.selectionEnd;
    var value = bodyInput.value;
    bodyInput.value = value.slice(0, start) + text + value.slice(end);
    var caret = start + text.length;
    bodyInput.setSelectionRange(caret, caret);
    bodyInput.focus();
    onInput();
  }

  function renderVariables(vars) {
    var box = document.getElementById('tpl-variables');
    box.innerHTML = '';
    vars.forEach(function (v) {
      // Colour carries the escaping policy, which is the thing an author needs
      // to predict: grey escapes, blue is a checked link, amber is verbatim.
      var cls = v.kind === 'url' ? 'text-bg-info'
        : v.kind === 'raw' ? 'text-bg-warning' : 'text-bg-secondary';
      var token = '{{ ' + v.name + ' }}';
      var badge = el('button', 'badge border-0 ' + cls, token);
      badge.type = 'button';
      badge.title = v.kind + (v.required ? ' — required in the body' : '');
      badge.onclick = function () { insertAtCursor(token); };
      box.appendChild(badge);
    });
  }

  async function select(t) {
    current = t;
    editor.classList.remove('d-none');
    previewCard.classList.add('d-none');
    document.getElementById('tpl-name').textContent = t.name;
    document.getElementById('tpl-key').textContent = t.key;
    document.getElementById('tpl-description').textContent = t.description;
    renderVariables(t.variables);
    status('', '');

    // The layout is wrapped around a message that supplies its own subject, so
    // an editable field here would look meaningful and change nothing.
    subjectInput.disabled = !!t.is_layout;
    document.getElementById('tpl-subject-inert').classList.toggle('d-none', !t.is_layout);

    try {
      var data = await request('GET', base());
      subjectInput.value = data.subject;
      bodyInput.value = data.body_html;
      markClean();
      document.getElementById('tpl-customised').classList.toggle('d-none', !data.customised);
      document.getElementById('tpl-default').classList.toggle('d-none', data.customised);
      // Reset only means something when there is a stored row to drop.
      document.getElementById('tpl-reset-btn').disabled = !data.customised;
    } catch (e) {
      status('text-danger', e.message);
    }
    renderList();
  }

  function onInput() {
    dirtyBadge.classList.toggle('d-none', !isDirty());
  }

  // ── Actions ───────────────────────────────────────────────────────────────

  async function save() {
    status('text-muted', '…');
    try {
      await request('PUT', base(), draft());
      markClean();
      status('text-success', document.getElementById('tpl-save-btn').dataset.savedText || 'Saved.');
      templates = await request('GET', '/api/admin/email-templates');
      var again = templates.filter(function (t) { return t.key === current.key; })[0];
      // Re-select to pick up the badge and the now-enabled Reset, but the
      // baseline is already clean so this cannot prompt.
      if (again) await select(again);
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
      showPreviewPane('html');
      previewCard.classList.remove('d-none');
      status('', '');
      // Otherwise the result renders below the fold and looks like nothing
      // happened.
      previewCard.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  function showPreviewPane(which) {
    var html = which === 'html';
    document.getElementById('tpl-preview-frame').classList.toggle('d-none', !html);
    document.getElementById('tpl-preview-text').classList.toggle('d-none', html);
    document.getElementById('tpl-view-html').classList.toggle('active', html);
    document.getElementById('tpl-view-text').classList.toggle('active', !html);
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
    var btn = document.getElementById('tpl-reset-btn');
    if (!window.confirm(btn.dataset.confirmText || 'Discard your edits for this language?')) {
      return;
    }
    status('text-muted', '…');
    try {
      await request('DELETE', base());
      templates = await request('GET', '/api/admin/email-templates');
      var again = templates.filter(function (t) { return t.key === current.key; })[0];
      // Clear the baseline first: the stored copy is gone, so whatever is in the
      // textarea is not "unsaved work" worth prompting about.
      markClean();
      if (again) await select(again);
      status('text-success', btn.dataset.resetText || 'Reset to the default copy.');
    } catch (e) {
      status('text-danger', e.message);
    }
  }

  subjectInput.oninput = onInput;
  bodyInput.oninput = onInput;
  document.getElementById('tpl-save-btn').onclick = save;
  document.getElementById('tpl-preview-btn').onclick = preview;
  document.getElementById('tpl-test-btn').onclick = testSend;
  document.getElementById('tpl-reset-btn').onclick = reset;
  document.getElementById('tpl-view-html').onclick = function () { showPreviewPane('html'); };
  document.getElementById('tpl-view-text').onclick = function () { showPreviewPane('text'); };

  var lastLocale = localeSelect.value;
  localeSelect.onchange = function () {
    if (current && !mayLeave()) {
      localeSelect.value = lastLocale;
      return;
    }
    lastLocale = localeSelect.value;
    if (current) select(current);
    else renderList();
  };

  // A tab close or reload still gets the browser's own prompt.
  window.addEventListener('beforeunload', function (e) {
    if (isDirty()) e.preventDefault();
  });

  request('GET', '/api/admin/email-templates')
    .then(function (data) {
      templates = data;
      renderList();
    })
    .catch(function (e) {
      list.innerHTML = '';
      list.appendChild(el('div', 'list-group-item text-danger small', e.message));
    });
})();
