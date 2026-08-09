/**
 * Product detail page — curator actions (edit / delete), rendered in place.
 *
 * Loaded only when the server set `can_manage_product`, so ordinary readers
 * never download it. That is a payload decision, not a security one: every
 * endpoint below re-checks `product.manage` in ProductUseCase, so forcing this
 * script into the page without the permission still yields 403s.
 *
 * Deliberately a reduced form — name, price, brand, materials, description,
 * photos. Slug and status belong to the full record in /admin/products.
 * Same-origin fetch satisfies the Origin-based CSRF check automatically.
 */
(function () {
  'use strict';

  var script = document.currentScript ||
    document.querySelector('script[src*="ferum-product-manage"]');
  var PRODUCT_ID = script && script.getAttribute('data-product-id');
  var PRODUCT_NAME = (script && script.getAttribute('data-product-name')) || '';
  if (!PRODUCT_ID) return;

  var editModalEl = document.getElementById('pdpEditModal');
  var delModalEl = document.getElementById('pdpDeleteModal');
  if (!editModalEl || !delModalEl) return;

  var vnd = new Intl.NumberFormat('vi-VN');
  var $ = function (id) { return document.getElementById(id); };
  function modal(el) { return bootstrap.Modal.getOrCreateInstance(el); }

  var allMaterials = [];
  var picked = [];        // material ids currently selected
  var pendingImages = []; // photos picked but not yet uploaded
  var refsLoaded = false;

  // ── Small shared helpers ──────────────────────────────────────────────────
  function getJSON(url) { return fetch(url, { headers: { Accept: 'application/json' } }); }
  function sendJSON(method, url, body) {
    return fetch(url, {
      method: method,
      headers: { 'Content-Type': 'application/json' },
      body: body != null ? JSON.stringify(body) : undefined,
    });
  }
  async function readError(res) {
    try {
      var b = await res.json();
      return Ferum.errorMessage(b) || Ferum.t('js-failed-save-changes');
    } catch (_) {
      return Ferum.t('js-failed-save-changes');
    }
  }

  function showError(msg) {
    var el = $('pdp-edit-error');
    el.textContent = msg;
    el.classList.remove('d-none');
    el.scrollIntoView({ block: 'nearest' });
  }
  function clearErrors() {
    var el = $('pdp-edit-error');
    el.textContent = '';
    el.classList.add('d-none');
    var form = $('pdp-edit-form');
    form.querySelectorAll('.is-invalid').forEach(function (n) { n.classList.remove('is-invalid'); });
    form.querySelectorAll('.invalid-feedback').forEach(function (n) { n.textContent = ''; });
  }
  function setFieldError(el, feedbackId, msg) {
    el.classList.add('is-invalid');
    var fb = $(feedbackId);
    if (fb) fb.textContent = msg;
  }

  // Prices are typed with whatever grouping the user is used to; strip it on
  // read, re-apply it on blur.
  function parsePrice(id) {
    var digits = ($(id).value || '').replace(/\D/g, '');
    return digits === '' ? null : parseInt(digits, 10);
  }
  function formatPriceField(id) {
    var digits = ($(id).value || '').replace(/\D/g, '');
    $(id).value = digits === '' ? '' : vnd.format(parseInt(digits, 10));
  }
  ['pdp-price-min', 'pdp-price-max'].forEach(function (id) {
    $(id).addEventListener('blur', function () { formatPriceField(id); });
    // The server renders raw digits; group them before the field is ever seen.
    formatPriceField(id);
  });

  // ── Reference data + current state ────────────────────────────────────────
  async function loadRefs() {
    if (refsLoaded) return;
    refsLoaded = true;

    var brandRes = await getJSON('/api/brands').catch(function () { return null; });
    if (brandRes && brandRes.ok) {
      var brands = (await brandRes.json()).data || [];
      var sel = $('pdp-brand');
      brands.forEach(function (b) {
        var o = document.createElement('option');
        o.value = b.id;
        o.textContent = b.name;
        sel.appendChild(o);
      });
      sel.value = sel.getAttribute('data-current') || '';
    }

    var matRes = await getJSON('/api/materials').catch(function () { return null; });
    if (matRes && matRes.ok) allMaterials = (await matRes.json()).data || [];

    // Start from what the product actually has, otherwise an empty picker is
    // ambiguous — it cannot tell "keep them" from "remove them all".
    var mineRes = await getJSON('/api/admin/products/' + PRODUCT_ID + '/materials')
      .catch(function () { return null; });
    if (mineRes && mineRes.ok) {
      picked = (await mineRes.json()).data || [];
      renderChips();
    }

    loadGallery();
  }

  // ── Materials: chip + typeahead (same interaction as the other forms) ─────
  function renderChips() {
    var box = $('pdp-mat-chips');
    box.innerHTML = '';
    picked.forEach(function (id) {
      var m = allMaterials.find(function (x) { return x.id === id; });
      var chip = document.createElement('span');
      chip.className = 'badge text-bg-secondary d-inline-flex align-items-center gap-1';
      chip.appendChild(document.createTextNode(m ? m.name : id));
      var rm = document.createElement('button');
      rm.type = 'button';
      rm.className = 'btn-close btn-close-white';
      rm.style.fontSize = '.5rem';
      rm.setAttribute('aria-label', Ferum.t('js-remove-material'));
      rm.addEventListener('click', function () {
        picked = picked.filter(function (x) { return x !== id; });
        renderChips();
      });
      chip.appendChild(rm);
      box.appendChild(chip);
    });
  }

  var matSearch = $('pdp-mat-search');
  var matResults = $('pdp-mat-results');
  var matWrap = $('pdp-mat-wrap');

  function showMatResults(term) {
    // Drop the menu upwards when the field sits too close to the bottom of the
    // viewport, so it never lands on the dialog's own buttons.
    var below = window.innerHeight - matWrap.getBoundingClientRect().bottom;
    matResults.classList.toggle('fr-typeahead-up', below < 240);

    var matches = allMaterials.filter(function (m) {
      return picked.indexOf(m.id) === -1 &&
        (!term || m.name.toLowerCase().indexOf(term.toLowerCase()) !== -1);
    }).slice(0, 12);

    matResults.innerHTML = '';
    if (!matches.length) {
      var note = document.createElement('div');
      note.className = 'list-group-item text-muted small';
      note.textContent = allMaterials.length
        ? Ferum.t('js-no-materials-found')
        : Ferum.t('js-no-materials-in-catalog');
      matResults.appendChild(note);
      matResults.classList.remove('d-none');
      return;
    }
    matches.forEach(function (m) {
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'list-group-item list-group-item-action';
      btn.textContent = m.name;
      btn.addEventListener('click', function () {
        picked.push(m.id);
        renderChips();
        matSearch.value = '';
        matResults.classList.add('d-none');
        matSearch.focus();
      });
      matResults.appendChild(btn);
    });
    matResults.classList.remove('d-none');
  }

  matSearch.addEventListener('input', function () { showMatResults(matSearch.value.trim()); });
  matSearch.addEventListener('focus', function () { showMatResults(matSearch.value.trim()); });
  matWrap.addEventListener('click', function () { matSearch.focus(); });
  document.addEventListener('click', function (e) {
    if (!matWrap.contains(e.target) && !matResults.contains(e.target)) {
      matResults.classList.add('d-none');
    }
  });

  // ── Photos ────────────────────────────────────────────────────────────────
  function thumb(src, onRemove) {
    var wrap = document.createElement('div');
    wrap.className = 'fr-thumb';
    var img = document.createElement('img');
    img.src = src;
    img.alt = '';
    wrap.appendChild(img);
    var rm = document.createElement('button');
    rm.type = 'button';
    rm.className = 'fr-thumb-remove';
    rm.textContent = '×';
    rm.title = Ferum.t('js-remove-photo');
    rm.setAttribute('aria-label', Ferum.t('js-remove-photo'));
    rm.addEventListener('click', onRemove);
    wrap.appendChild(rm);
    return wrap;
  }

  var savedMedia = [];

  function renderGallery() {
    var g = $('pdp-gallery');
    g.innerHTML = '';
    // Already stored — removing one deletes it straight away.
    savedMedia.forEach(function (md) {
      g.appendChild(thumb(md.url, function () { deleteMedia(md.id); }));
    });
    // Picked in this session — uploaded when the form is saved.
    pendingImages.forEach(function (p, i) {
      g.appendChild(thumb(p.url, function () {
        URL.revokeObjectURL(pendingImages[i].url);
        pendingImages.splice(i, 1);
        renderGallery();
      }));
    });
  }

  async function loadGallery() {
    var res = await getJSON('/api/admin/products/' + PRODUCT_ID + '/media')
      .catch(function () { return null; });
    savedMedia = res && res.ok ? ((await res.json()).data || []) : [];
    renderGallery();
  }

  async function deleteMedia(mediaId) {
    var res = await fetch('/api/admin/products/' + PRODUCT_ID + '/media/' + mediaId, { method: 'DELETE' });
    if (!res.ok && res.status !== 204) { showError(await readError(res)); return; }
    savedMedia = savedMedia.filter(function (m) { return m.id !== mediaId; });
    renderGallery();
  }

  $('pdp-images').addEventListener('change', function (e) {
    Array.prototype.slice.call(e.target.files || []).forEach(function (f) {
      pendingImages.push({ file: f, url: URL.createObjectURL(f) });
    });
    e.target.value = '';
    renderGallery();
  });

  /// Upload every staged photo. Returns how many failed; the product itself is
  /// saved either way.
  async function uploadPending() {
    var failed = 0;
    for (var i = 0; i < pendingImages.length; i++) {
      var fd = new FormData();
      fd.append('image', pendingImages[i].file);
      try {
        var res = await fetch('/api/admin/products/' + PRODUCT_ID + '/media', { method: 'POST', body: fd });
        if (!res.ok) failed++;
      } catch (_) {
        failed++;
      }
    }
    pendingImages.forEach(function (p) { URL.revokeObjectURL(p.url); });
    pendingImages = [];
    return failed;
  }

  // ── Save ──────────────────────────────────────────────────────────────────
  editModalEl.addEventListener('show.bs.modal', loadRefs);
  editModalEl.addEventListener('shown.bs.modal', function () { $('pdp-name').focus(); });

  $('pdp-edit-form').addEventListener('input', function (e) {
    if (e.target.classList && e.target.classList.contains('is-invalid')) {
      e.target.classList.remove('is-invalid');
    }
  });

  $('pdp-edit-form').addEventListener('submit', async function (ev) {
    ev.preventDefault();
    clearErrors();

    var name = $('pdp-name').value.trim();
    if (!name) setFieldError($('pdp-name'), 'pdp-name-feedback', Ferum.t('js-enter-product-name'));

    var min = parsePrice('pdp-price-min'), max = parsePrice('pdp-price-max');
    if (min != null && max != null && min > max) {
      setFieldError($('pdp-price-min'), 'pdp-price-feedback', Ferum.t('js-price-from-exceeds-to'));
      $('pdp-price-max').classList.add('is-invalid');
    }

    var firstBad = $('pdp-edit-form').querySelector('.is-invalid');
    if (firstBad) { firstBad.focus(); firstBad.scrollIntoView({ block: 'center' }); return; }

    var btn = $('pdp-save');
    btn.disabled = true;
    try {
      // Nullable fields are omitted rather than sent as null when empty.
      // `UpdateProductRequest` types them `Option<Option<T>>`, and with plain
      // serde a JSON null deserializes to the OUTER None — i.e. "leave
      // unchanged", not "clear". Sending null would therefore be a silent
      // no-op; omitting it says the same thing honestly. Clearing a value back
      // to empty is not expressible through this endpoint at all (that needs a
      // `double_option` deserializer server-side).
      var patch = { name: name };
      var brand = $('pdp-brand').value;
      if (brand) patch.brand_id = brand;
      if (min != null) patch.price_min = min;
      if (max != null) patch.price_max = max;
      var desc = $('pdp-description').value.trim();
      if (desc) patch.description_md = desc;
      var res = await sendJSON('PATCH', '/api/admin/products/' + PRODUCT_ID, patch);
      if (!res.ok) { showError(await readError(res)); return; }

      // Always sent — the picker was pre-filled with the current set, so an
      // empty list is a deliberate "remove all", not "leave alone".
      await sendJSON('POST', '/api/admin/products/' + PRODUCT_ID + '/materials', { material_ids: picked });

      if (pendingImages.length) await uploadPending();

      // Reload rather than patching the page by hand: the heading, gallery,
      // price line, brand and material chips are all server-rendered, and
      // re-deriving each one here is exactly how the two drift apart.
      window.location.reload();
    } catch (e) {
      showError(Ferum.t('js-network-error'));
    } finally {
      btn.disabled = false;
    }
  });

  // ── Delete ────────────────────────────────────────────────────────────────
  function depRow(label, count) {
    var li = document.createElement('li');
    li.className = 'list-group-item d-flex justify-content-between px-0' + (count === 0 ? ' text-muted' : '');
    var l = document.createElement('span');
    l.textContent = label;
    var c = document.createElement('span');
    c.className = 'fw-semibold';
    c.textContent = count;
    li.appendChild(l);
    li.appendChild(c);
    return li;
  }

  $('pdp-delete-open').addEventListener('click', async function () {
    var nameEl = $('pdp-del-name');
    nameEl.textContent = '';
    nameEl.appendChild(document.createTextNode(Ferum.t('js-delete-product-confirm') + ' '));
    var strong = document.createElement('strong');
    strong.textContent = PRODUCT_NAME;
    nameEl.appendChild(strong);

    $('pdp-del-dependents').innerHTML = '';
    $('pdp-del-blocked').classList.add('d-none');
    $('pdp-del-safe').classList.add('d-none');
    $('pdp-del-confirm').disabled = true;
    modal(delModalEl).show();

    var res = await getJSON('/api/admin/products/' + PRODUCT_ID + '/dependents')
      .catch(function () { return null; });
    if (!res || !res.ok) {
      $('pdp-del-blocked').textContent = res ? await readError(res) : Ferum.t('js-network-error');
      $('pdp-del-blocked').classList.remove('d-none');
      return;
    }
    var d = (await res.json()).data || {};

    // `js-` prefixed throughout: the client dictionary in <meta name="ferum-i18n">
    // only carries that namespace, so a `ui-` key here would render as its own name.
    var list = $('pdp-del-dependents');
    list.appendChild(depRow(Ferum.t('js-reviews'), d.reviews));
    list.appendChild(depRow(Ferum.t('js-photos'), d.media));
    list.appendChild(depRow(Ferum.t('js-material-links'), d.materials));

    if (d.can_hard_delete) {
      $('pdp-del-safe').textContent = Ferum.t('js-product-delete-permanent-warning');
      $('pdp-del-safe').classList.remove('d-none');
      $('pdp-del-confirm').disabled = false;
    } else {
      $('pdp-del-blocked').textContent = Ferum.t('js-product-delete-blocked', { count: d.reviews });
      $('pdp-del-blocked').classList.remove('d-none');
      $('pdp-del-confirm').disabled = true;
    }
  });

  $('pdp-del-confirm').addEventListener('click', async function () {
    var res = await fetch('/api/admin/products/' + PRODUCT_ID, { method: 'DELETE' });
    if (!res.ok && res.status !== 204) {
      $('pdp-del-blocked').textContent = await readError(res);
      $('pdp-del-blocked').classList.remove('d-none');
      return;
    }
    // The page being viewed no longer exists — leave it.
    window.location.href = '/catalog';
  });

  // Absent for contributors: archiving is a status change, which the use case
  // refuses for them, so the template does not render the button.
  var archiveBtn = $('pdp-del-archive');
  if (archiveBtn) {
    archiveBtn.addEventListener('click', async function () {
      var res = await sendJSON('PATCH', '/api/admin/products/' + PRODUCT_ID, { status: 'archived' });
      if (!res.ok) {
        $('pdp-del-blocked').textContent = await readError(res);
        $('pdp-del-blocked').classList.remove('d-none');
        return;
      }
      window.location.reload();
    });
  }
}());
