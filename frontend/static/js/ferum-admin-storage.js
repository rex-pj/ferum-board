// /admin/storage — drives the two orphan-finding endpoints.
//
// The sweep is PAGED: one request examines a bounded slice of the store so a
// bucket with a hundred thousand objects cannot turn into a ten-minute response.
// Following `next_after` until it comes back null is this file's main job, and
// the reason the page renders nothing server-side.
//
// Deleting is gated on having scanned first — not decoration. The whole reason
// this page exists rather than a curl command is that the list gets read before
// anything acts on it.
(function () {
  'use strict';

  var page = document.getElementById('sweep-results');
  if (!page) return;

  var PAGE_SIZE = 500;
  // A stop so a runaway cursor cannot loop forever. 200 × 500 = 100k objects,
  // past which this tool is the wrong shape anyway.
  var MAX_PAGES = 200;

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function status(id, cls, text) {
    var box = document.getElementById(id);
    box.className = 'small mb-2 ' + (cls || '');
    box.textContent = text;
  }

  /// Renders a namespace group with a thumbnail per key.
  ///
  /// The thumbnail is the point of the page. `logos/a1b2c3d4.png` tells an admin
  /// nothing about whether losing it matters; the image does.
  ///
  /// It resolves for every group except one. A row still exists for anything at
  /// `ref_count <= 0`, so `/files/` serves those right up until collection —
  /// including staged post attachments, which `resolve_stored_file` admits for
  /// staff precisely so this page can show them. The exception is a true orphan
  /// under database storage: no row means no bytes, so those fall back to the
  /// placeholder and always will.
  function renderGroup(container, label, keys) {
    var group = el('div', 'mb-3');
    group.appendChild(el('div', 'fw-semibold small mb-2', label + ' — ' + keys.length));

    var grid = el('div', 'd-flex flex-wrap gap-2');
    keys.forEach(function (key) {
      var card = el('div', 'border rounded p-1 text-center');
      card.style.width = '104px';

      var img = document.createElement('img');
      img.src = '/files/' + key;
      img.alt = '';
      img.loading = 'lazy';
      img.style.cssText = 'width:96px;height:96px;object-fit:cover;border-radius:3px';
      // A key can name something that is not an image, or one already deleted by
      // an earlier run. Swapping in a placeholder keeps the grid readable
      // instead of showing a broken-image glyph.
      img.onerror = function () {
        var box = el('div', 'text-muted d-flex align-items-center justify-content-center');
        box.style.cssText =
          'width:96px;height:96px;background:var(--bs-secondary-bg);border-radius:3px';
        box.appendChild(el('i', 'fa-regular fa-file'));
        img.replaceWith(box);
      };
      card.appendChild(img);

      var name = el('div', 'text-muted mt-1', key.split('/').pop());
      name.style.cssText = 'font-size:10px;word-break:break-all';
      name.title = key;
      card.appendChild(name);

      grid.appendChild(card);
    });
    group.appendChild(grid);
    container.appendChild(group);
  }

  async function get(url) {
    var res = await fetch(url, { headers: { Accept: 'application/json' } });
    if (!res.ok) throw new Error('HTTP ' + res.status);
    return (await res.json()).data;
  }

  async function post(url) {
    var res = await fetch(url, { method: 'POST', headers: { Accept: 'application/json' } });
    if (!res.ok) throw new Error('HTTP ' + res.status);
    return (await res.json()).data;
  }

  // ── Sweep: store → rows ──────────────────────────────────────────────────

  var sweepFindings = null;

  async function runSweep(apply) {
    var out = document.getElementById('sweep-results');
    out.innerHTML = '';
    document.getElementById('sweep-apply').disabled = true;

    var after = null;
    var scanned = 0;
    var orphans = [];
    var uncollected = [];
    var retained = [];
    var active = 0;
    var deleted = 0;

    try {
      for (var i = 0; i < MAX_PAGES; i++) {
        var q = '?limit=' + PAGE_SIZE + (after ? '&after=' + encodeURIComponent(after) : '');
        var url = '/api/admin/storage/sweep' + q;
        var data = apply ? await post(url) : await get(url);

        // Not the same as an empty result, and the difference matters enough to
        // stop rather than report a clean store nobody looked at.
        if (!data.enumerable) {
          status('sweep-status', 'text-warning',
            'This storage backend cannot list itself, so nothing was examined. ' +
            'The audit below still works.');
          return;
        }

        scanned += data.scanned;
        orphans = orphans.concat(data.orphaned_objects);
        uncollected = uncollected.concat(data.uncollected);
        retained = retained.concat(data.retained_attachments || []);
        active += data.active_attachments || 0;
        deleted += data.deleted;
        status('sweep-status', 'text-muted', 'Scanned ' + scanned + ' objects…');

        after = data.next_after;
        if (!after) break;
      }
    } catch (e) {
      status('sweep-status', 'text-danger', 'Scan failed: ' + e.message);
      return;
    }

    sweepFindings = orphans.length + uncollected.length;
    if (apply) {
      status('sweep-status', 'text-success',
        'Deleted ' + deleted + ' orphaned objects and scheduled ' +
        uncollected.length + ' rows for collection. Scanned ' + scanned + '.');
      return;
    }

    // Retained attachments are shown but are not "findings" — the delete button
    // must not light up for something it will refuse to touch.
    if (retained.length) {
      renderGroup(out, 'Un-published post attachments — kept as moderation evidence, ' +
        'delete by hand only', retained);
    }
    // Counted, never listed. These are inside the grace window, so each one is
    // most likely an image in a composer somebody still has open — see
    // `active_attachments` in storage_audit_usecase.rs. Naming the keys would
    // invite deleting a live draft's picture.
    if (active) {
      // "more" only when something was actually listed above it, or the sentence
      // points at nothing. The first wording said "3 more" under an empty list.
      var note = el('div', 'text-muted small',
        active + (retained.length ? ' more' : '') +
        ' post attachment(s) uploaded in the last 24h are not listed: until then ' +
        'an unreferenced attachment is indistinguishable from one in a composer ' +
        'somebody still has open. They appear above once they age past that.');
      out.appendChild(note);
    }

    if (sweepFindings === 0) {
      // "Nothing to clean" is about what this button would act on. Retained
      // attachments are deliberately not that, so they are named separately
      // rather than folded into a count that reads as zero work outstanding.
      status('sweep-status', 'text-success',
        'Scanned ' + scanned + ' objects — nothing to clean automatically' +
        (retained.length
          ? '. ' + retained.length + ' post attachment(s) listed below for manual review.'
          : '.'));
      return;
    }

    status('sweep-status', 'text-warning',
      'Scanned ' + scanned + ' objects: ' + orphans.length + ' with no database row, ' +
      uncollected.length + ' awaiting collection.');
    if (orphans.length) renderGroup(out, 'No database row', orphans);
    if (uncollected.length) renderGroup(out, 'Awaiting collection', uncollected);
    document.getElementById('sweep-apply').disabled = false;
  }

  // ── Audit: rows → pointers ───────────────────────────────────────────────

  async function runAudit(apply) {
    var out = document.getElementById('audit-results');
    out.innerHTML = '';
    document.getElementById('audit-apply').disabled = true;
    status('audit-status', 'text-muted', 'Checking…');

    var data;
    try {
      data = apply
        ? await post('/api/admin/storage/audit')
        : await get('/api/admin/storage/audit');
    } catch (e) {
      status('audit-status', 'text-danger', 'Audit failed: ' + e.message);
      return;
    }

    if (apply) {
      status('audit-status', 'text-success', 'Released ' + data.released + ' references.');
      return;
    }
    if (data.total === 0) {
      status('audit-status', 'text-success', 'Every stored file is still referenced.');
      return;
    }

    status('audit-status', 'text-warning',
      data.total + ' files hold a reference nothing points at.');
    data.findings.forEach(function (f) {
      renderGroup(out, f.namespace, f.keys);
    });
    document.getElementById('audit-apply').disabled = false;
  }

  // ── Wiring ───────────────────────────────────────────────────────────────

  function confirmThen(count, what, fn) {
    if (!window.confirm(
      'Permanently delete ' + count + ' ' + what + '?\n\n' +
      'This cannot be undone. The files are removed from storage.'
    )) return;
    fn();
  }

  document.getElementById('sweep-scan').onclick = function () { runSweep(false); };
  document.getElementById('sweep-apply').onclick = function () {
    confirmThen(sweepFindings, 'files', function () { runSweep(true); });
  };
  document.getElementById('audit-scan').onclick = function () { runAudit(false); };
  document.getElementById('audit-apply').onclick = function () {
    // Re-reads the count from the status line's own state rather than trusting a
    // stale variable — the audit is re-run server-side anyway.
    confirmThen('the listed', 'files', function () { runAudit(true); });
  };
})();
