/**
 * Admin catalog — drives /admin/products (tabs: Products / Materials / Brands).
 * Reads /api/admin/products, /api/materials, /api/brands; writes via /api/admin/*.
 * Same-origin fetch satisfies the Origin-based CSRF check automatically.
 */
(function () {
  'use strict';

  var state = {
    materials: [], brands: [], selectedMaterials: [],
    // Catalogue taxonomy + how many products still have no category. The
    // unfiled count is the size of the remaining manual queue.
    pcats: [], unfiled: 0,
    page: 1, perPage: 20, total: 0, editing: null,
    // Images picked while creating a product. There is no id to upload against
    // until the product exists, so they wait here and are sent after the POST.
    pendingFiles: [],
    // Product currently shown in the delete dialog.
    deleting: null,
  };

  function $(id) { return document.getElementById(id); }

  // Was a local copy predating the shared helper being quote-safe. Now that
  // Ferum.escapeHtml escapes quotes too, there is one implementation to audit
  // instead of two that can drift apart.
  var escapeHtml = Ferum.escapeHtml;

  function slugify(s) {
    return String(s || '')
      .normalize('NFD').replace(/[̀-ͯ]/g, '')
      .replace(/đ/g, 'd').replace(/Đ/g, 'D')
      .toLowerCase().trim()
      .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  }

  function showStatus(msg, kind) {
    var el = $('catalogStatus');
    el.className = 'alert alert-' + (kind || 'info');
    el.textContent = msg;
    el.classList.remove('d-none');
    if (kind === 'success') setTimeout(function () { el.classList.add('d-none'); }, 2500);
  }

  async function readError(res) {
    try { var b = await res.json(); return Ferum.errorMessage(b) || ('Error (' + res.status + ')'); }
    catch (e) { return 'Error (' + res.status + ')'; }
  }

  // Transport goes through FerumApi.http, which is where the headers and the
  // verb dispatch live. This file and ferum-product-manage.js each used to
  // carry a private getJSON/sendJSON pair — sendJSON byte-identical between
  // them — plus bare fetch() calls that bypassed both.
  //
  // FerumApi.http is read at each call rather than captured in a local here:
  // both files are pulled in by templates whose script order a theme can
  // change, and a load-time capture would depend on that order.
  function strOrNull(id) { var v = $(id).value.trim(); return v === '' ? null : v; }
  function modal(id) { return bootstrap.Modal.getOrCreateInstance($(id)); }

  // ── Segmented controls (radio button groups) ─────────────────────────────
  function radioValue(name) {
    var el = document.querySelector('input[name="' + name + '"]:checked');
    return el ? el.value : '';
  }
  function setRadio(name, value) {
    var el = document.querySelector('input[name="' + name + '"][value="' + value + '"]');
    if (el) el.checked = true;
  }

  // ── Prices ───────────────────────────────────────────────────────────────
  // Typed as free text so "1.500.000", "1 500 000" and "1500000" all work.
  // Separators are stripped on read and re-applied on blur.
  function parsePrice(id) {
    var digits = $(id).value.replace(/\D/g, '');
    return digits === '' ? null : parseInt(digits, 10);
  }
  function formatPriceField(el) {
    var digits = el.value.replace(/\D/g, '');
    el.value = digits === '' ? '' : Ferum.formatNumber(parseInt(digits, 10));
  }

  // ── Modal-scoped feedback ────────────────────────────────────────────────
  // A failed save has to report inside the dialog it happened in. The page-level
  // #catalogStatus alert is behind the backdrop whenever a modal is open, so
  // anything routed there while saving is simply never seen.
  function showFormError(id, msg) {
    var el = $(id);
    if (!el) return;
    el.textContent = msg;
    el.classList.remove('d-none');
    el.scrollIntoView({ block: 'nearest' });
  }
  function hideFormError(id) {
    var el = $(id);
    if (el) { el.textContent = ''; el.classList.add('d-none'); }
  }

  /// Mark one field invalid and write its message into the paired feedback node.
  function setFieldError(inputId, feedbackId, msg) {
    var input = $(inputId), fb = $(feedbackId);
    if (input) input.classList.add('is-invalid');
    if (fb) fb.textContent = msg;
  }

  /// Wipe every field-level error inside a form, plus its dialog-level alert.
  function clearErrors(formId, alertId) {
    var form = $(formId);
    if (form) {
      form.querySelectorAll('.is-invalid').forEach(function (el) { el.classList.remove('is-invalid'); });
      form.querySelectorAll('.invalid-feedback').forEach(function (el) { el.textContent = ''; });
    }
    hideFormError(alertId);
  }

  /// Focus the first field carrying an error so the fix starts where it should,
  /// revealing it first if it lives in a collapsed disclosure.
  function focusFirstInvalid(formId) {
    var el = $(formId).querySelector('.is-invalid');
    if (!el) return false;
    if (el.closest('.d-none')) el.closest('.d-none').classList.remove('d-none');
    el.focus();
    el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    return true;
  }

  // ── Tabs ─────────────────────────────────────────────────────────────────
  function initTabs() {
    var btns = document.querySelectorAll('#catalogTabs [data-tab]');
    Array.prototype.forEach.call(btns, function (btn) {
      btn.addEventListener('click', function () {
        Array.prototype.forEach.call(btns, function (b) { b.classList.remove('active'); });
        btn.classList.add('active');
        var tab = btn.getAttribute('data-tab');
        document.querySelectorAll('[data-panel]').forEach(function (p) {
          p.classList.toggle('d-none', p.getAttribute('data-panel') !== tab);
        });
      });
    });
  }

  // ── Products ─────────────────────────────────────────────────────────────
  function typeBadge(t) {
    var map = { furniture: ['primary', 'Furniture'], material: ['info', 'Material'], room: ['secondary', 'Room'] };
    var m = map[t] || ['secondary', t];
    return '<span class="badge text-bg-' + m[0] + '">' + escapeHtml(m[1]) + '</span>';
  }
  function statusBadge(s) {
    var map = { published: ['success', 'Published'], draft: ['warning', 'Pending'], archived: ['secondary', 'Archived'] };
    var m = map[s] || ['secondary', s];
    return '<span class="badge text-bg-' + m[0] + '">' + escapeHtml(m[1]) + '</span>';
  }
  function priceRange(p) {
    if (p.price_min == null && p.price_max == null) return '<span class="text-muted">—</span>';
    if (p.price_min != null && p.price_max != null && p.price_min !== p.price_max)
      return Ferum.formatNumber(p.price_min) + ' – ' + Ferum.formatNumber(p.price_max);
    return Ferum.formatNumber(p.price_min != null ? p.price_min : p.price_max);
  }
  function brandName(id) {
    if (!id) return '<span class="text-muted">—</span>';
    var b = state.brands.find(function (x) { return x.id === id; });
    return b ? escapeHtml(b.name) : '<span class="text-muted">—</span>';
  }

  // Resolved off the server-rendered picker rather than a second fetch — the
  // option list is already on the page, and it is the same list the form writes
  // back, so the two can't disagree.
  //
  // An unfiled product is badged, not dashed like a missing brand: a product
  // with no forum category is excluded from the public category filter, so this
  // is a gap to close rather than merely a blank.
  function categoryName(id) {
    if (!id) {
      var sel = $('pCategory');
      var blank = sel ? sel.querySelector('option[value=""]') : null;
      var label = blank ? blank.textContent.trim() : 'Uncategorised';
      return '<span class="badge text-bg-warning-subtle border border-warning-subtle fw-normal">' + escapeHtml(label) + '</span>';
    }
    var c = state.pcats.find(function (x) { return x.id === id; });
    return c ? escapeHtml(c.name) : '<span class="text-muted">—</span>';
  }

  function renderProducts(products) {
    var tbody = $('productRows');
    if (!tbody) return;
    if (!products.length) { tbody.innerHTML = '<tr><td colspan="6" class="text-center text-muted py-4">No products yet.</td></tr>'; return; }
    tbody.innerHTML = products.map(function (p) {
      var moderate = p.status === 'draft'
        ? '<button class="btn btn-sm btn-success me-1" data-approve="' + p.id + '" title="Approve"><i class="fa-solid fa-check"></i></button>' +
          '<button class="btn btn-sm btn-outline-warning me-1" data-reject="' + p.id + '" data-name="' + escapeHtml(p.name) + '" title="Reject"><i class="fa-solid fa-ban"></i></button>'
        : '';
      return '<tr>' +
        '<td><div class="fw-semibold">' + escapeHtml(p.name) + ' ' + typeBadge(p.product_type) + '</div>' +
        '<div class="text-muted small">/' + escapeHtml(p.slug) + (p.style ? ' · ' + escapeHtml(p.style) : '') + '</div></td>' +
        '<td>' + priceRange(p) + '</td>' +
        '<td class="small">' + brandName(p.brand_id) + '</td>' +
        '<td class="small">' + categoryName(p.category_id) + '</td>' +
        '<td>' + statusBadge(p.status) + '</td>' +
        '<td class="text-end">' + moderate +
        '<button class="btn btn-sm btn-outline-secondary me-1" data-edit="' + p.id + '"><i class="fa-solid fa-pen"></i></button>' +
        '<button class="btn btn-sm btn-outline-danger" data-del="' + p.id + '" data-name="' + escapeHtml(p.name) + '"><i class="fa-solid fa-trash"></i></button>' +
        '</td></tr>';
    }).join('');
    tbody.querySelectorAll('[data-edit]').forEach(function (b) { b.addEventListener('click', function () { var p = products.find(function (x) { return x.id === b.getAttribute('data-edit'); }); if (p) openProduct(p); }); });
    tbody.querySelectorAll('[data-del]').forEach(function (b) { b.addEventListener('click', function () { deleteProduct(b.getAttribute('data-del'), b.getAttribute('data-name')); }); });
    tbody.querySelectorAll('[data-approve]').forEach(function (b) { b.addEventListener('click', function () { setProductStatus(b.getAttribute('data-approve'), 'published', 'Product published.'); }); });
    tbody.querySelectorAll('[data-reject]').forEach(function (b) { b.addEventListener('click', function () { if (confirm('Reject "' + b.getAttribute('data-name') + '"?')) setProductStatus(b.getAttribute('data-reject'), 'archived', 'Product archived.'); }); });
  }

  // Numbered pagination mirroring the server `paginate` macro (first · left
  // ellipsis when page > 4 · ±2 window · right ellipsis · last · prev/next),
  // so the catalog footer matches every other admin list page.
  function pagerItem(label, page, opts) {
    opts = opts || {};
    if (opts.active) return '<li class="page-item active" aria-current="page"><span class="page-link">' + label + '</span></li>';
    if (opts.disabled) return '<li class="page-item disabled" aria-hidden="true"><span class="page-link">' + label + '</span></li>';
    return '<li class="page-item"><button type="button" class="page-link" data-page="' + page + '"' +
      (opts.ariaLabel ? ' aria-label="' + opts.ariaLabel + '"' : '') + '>' + label + '</button></li>';
  }
  function renderProductsPager(totalPages) {
    var ul = $('catalogPager');
    if (!ul) return;
    if (totalPages <= 1) { ul.innerHTML = ''; return; }
    var page = state.page;
    var chevL = '<i class="fa-solid fa-chevron-left" aria-hidden="true" style="font-size:.65rem"></i>';
    var chevR = '<i class="fa-solid fa-chevron-right" aria-hidden="true" style="font-size:.65rem"></i>';
    var html = page > 1 ? pagerItem(chevL, page - 1, { ariaLabel: 'Previous page' })
                        : pagerItem(chevL, 0, { disabled: true });
    html += pagerItem('1', 1, { active: page === 1 });
    if (page > 4) html += pagerItem('&hellip;', 0, { disabled: true });
    for (var p = 2; p <= totalPages - 1; p++) {
      if (p >= page - 2 && p <= page + 2) html += pagerItem(String(p), p, { active: p === page });
    }
    if (page < totalPages - 3) html += pagerItem('&hellip;', 0, { disabled: true });
    html += pagerItem(String(totalPages), totalPages, { active: page === totalPages });
    html += page < totalPages ? pagerItem(chevR, page + 1, { ariaLabel: 'Next page' })
                              : pagerItem(chevR, 0, { disabled: true });
    ul.innerHTML = html;
  }

  async function loadProducts() {
    var params = new URLSearchParams();
    params.set('page', state.page); params.set('per_page', state.perPage);
    var q = $('filterQuery').value.trim(), t = $('filterType').value, s = $('filterStatus').value;
    // `none` is a real value here, not an absent one — it selects products with
    // no forum category, which is the queue a curator works through.
    var c = $('filterCategory') ? $('filterCategory').value : '';
    if (q) params.set('q', q); if (t) params.set('type', t); if (s) params.set('status', s);
    if (c) params.set('category_id', c);
    var res = await FerumApi.http.get('/api/admin/products?' + params.toString());
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    var body = await res.json();
    renderProducts(body.data || []);
    state.total = body.meta ? body.meta.total : 0;
    var totalPages = Math.max(1, Math.ceil(state.total / state.perPage));
    $('catalogMeta').textContent = state.total + ' products · page ' + state.page + ' / ' + totalPages;
    renderProductsPager(totalPages);
  }

  async function setProductStatus(id, status, ok) {
    var res = await FerumApi.http.send('PATCH', '/api/admin/products/' + id, { status: status });
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    showStatus(ok, 'success'); loadProducts();
  }

  // ── Product modal: materials chip picker ──────────────────────────────────
  function renderMatChips() {
    $('pMatChips').innerHTML = state.selectedMaterials.map(function (id) {
      var m = state.materials.find(function (x) { return x.id === id; });
      var name = m ? m.name : id;
      return '<span class="badge text-bg-secondary d-inline-flex align-items-center gap-1">' + escapeHtml(name) +
        '<button type="button" class="btn-close btn-close-white" style="font-size:.5rem" data-rmmat="' + id + '" aria-label="Remove"></button></span>';
    }).join('');
    $('pMatChips').querySelectorAll('[data-rmmat]').forEach(function (b) {
      b.addEventListener('click', function () {
        state.selectedMaterials = state.selectedMaterials.filter(function (x) { return x !== b.getAttribute('data-rmmat'); });
        renderMatChips();
      });
    });
  }
  /// Drop the menu upwards when the field is too close to the bottom of the
  /// viewport, so it never lands on top of the dialog's own buttons.
  function placeMatResults() {
    var box = $('pMatResults');
    var below = window.innerHeight - $('pMatWrap').getBoundingClientRect().bottom;
    box.classList.toggle('fr-typeahead-up', below < 240);
  }

  function matResults(term) {
    var box = $('pMatResults');
    placeMatResults();
    var picked = state.selectedMaterials;
    var matches = state.materials.filter(function (m) {
      return picked.indexOf(m.id) === -1 && (!term || m.name.toLowerCase().indexOf(term.toLowerCase()) !== -1);
    }).slice(0, 12);
    if (!matches.length) { box.classList.add('d-none'); return; }
    box.innerHTML = matches.map(function (m) {
      return '<button type="button" class="list-group-item list-group-item-action py-1" data-addmat="' + m.id + '">' +
        escapeHtml(m.name) + ' <span class="text-muted small">(' + escapeHtml(m.category) + ')</span></button>';
    }).join('');
    box.querySelectorAll('[data-addmat]').forEach(function (b) {
      b.addEventListener('click', function () {
        state.selectedMaterials.push(b.getAttribute('data-addmat'));
        renderMatChips();
        $('pMatSearch').value = '';
        box.classList.add('d-none');
        $('pMatSearch').focus();
      });
    });
    box.classList.remove('d-none');
  }
  function initMatPicker() {
    var search = $('pMatSearch');
    search.addEventListener('input', function () { matResults(search.value.trim()); });
    search.addEventListener('focus', function () { matResults(search.value.trim()); });
    document.addEventListener('click', function (e) {
      if (!$('pMatWrap').contains(e.target) && e.target !== search) $('pMatResults').classList.add('d-none');
    });
  }

  // ── Product modal open/reset/submit ───────────────────────────────────────
  function clearPendingFiles() {
    state.pendingFiles.forEach(function (p) { URL.revokeObjectURL(p.url); });
    state.pendingFiles = [];
  }

  /// Repaint the `/p/<slug>` helper line under the Name field.
  function renderSlugPreview() {
    var slug = $('pSlug').value;
    $('pSlugPreview').textContent = slug ? '/p/' + slug : '—';
  }

  function resetProductForm() {
    state.editing = null;
    state.selectedMaterials = [];
    $('productModalTitle').textContent = 'New product';
    $('productId').value = '';
    ['pName', 'pSlug', 'pStyle', 'pPriceMin', 'pPriceMax', 'pOrigin', 'pDescription', 'pMatSearch'].forEach(function (id) { $(id).value = ''; });
    setRadio('pType', 'furniture');
    setRadio('pStatus', 'published');
    $('pBrand').value = '';
    if ($('pCategory')) $('pCategory').value = '';
    // Slug starts collapsed behind the helper line and only opens on request.
    $('pSlugEditWrap').classList.add('d-none');
    $('pSlugEdit').classList.remove('d-none');
    _slugManual = false;
    renderSlugPreview();
    clearErrors('productForm', 'productFormError');
    renderMatChips();
    clearPendingFiles();
    $('pGallery').innerHTML = '';
    $('pImageInput').value = '';
    renderPendingGallery();
  }

  var _slugManual = false;
  // Set by any edit to the product form, cleared on open and on a successful
  // save. Drives the discard confirmation when the dialog is dismissed.
  var _productDirty = false;

  function openProduct(p) {
    resetProductForm();
    state.editing = p.id;
    $('productModalTitle').textContent = 'Edit product';
    $('productId').value = p.id;
    $('pName').value = p.name || '';
    $('pSlug').value = p.slug || '';
    // Slug is immutable once the product exists — show it, offer no way in.
    $('pSlugEdit').classList.add('d-none');
    $('pSlugEditWrap').classList.add('d-none');
    renderSlugPreview();
    setRadio('pType', p.product_type || 'furniture');
    setRadio('pStatus', p.status || 'draft');
    $('pBrand').value = p.brand_id || '';
    if ($('pCategory')) $('pCategory').value = p.category_id || '';
    $('pStyle').value = p.style || '';
    $('pPriceMin').value = p.price_min != null ? Ferum.formatNumber(p.price_min) : '';
    $('pPriceMax').value = p.price_max != null ? Ferum.formatNumber(p.price_max) : '';
    $('pOrigin').value = p.origin || '';
    $('pDescription').value = p.description_md || '';
    loadProductMaterials(p.id);
    loadMedia(p.id);
    modal('productModal').show();
  }

  /// Validate the product form in place. Returns false and marks the offending
  /// fields rather than relying on native bubbles, which cannot point at a field
  /// hidden inside the collapsed slug disclosure.
  function validateProduct() {
    clearErrors('productForm', 'productFormError');
    var name = $('pName').value.trim();
    if (!name) setFieldError('pName', 'pNameFeedback', 'Enter a product name.');

    // Only meaningful on create — on edit the slug is immutable and not sent.
    if (!state.editing) {
      var slug = $('pSlug').value.trim();
      if (!slug) {
        setFieldError('pSlug', 'pSlugFeedback', 'A URL slug is required. It is filled in from the name automatically.');
      } else if (!/^[a-z0-9-]+$/.test(slug)) {
        setFieldError('pSlug', 'pSlugFeedback', 'Use lowercase letters, numbers and hyphens only.');
      }
    }

    var min = parsePrice('pPriceMin'), max = parsePrice('pPriceMax');
    if (min != null && max != null && min > max) {
      setFieldError('pPriceMin', 'pPriceFeedback', '"Price from" cannot be greater than "price to".');
      $('pPriceMax').classList.add('is-invalid');
    }

    return !focusFirstInvalid('productForm');
  }

  async function submitProduct(ev) {
    ev.preventDefault();
    if (!validateProduct()) return;
    var editing = state.editing;
    var mats = state.selectedMaterials.slice();
    var submitBtn = $('productSubmit');
    submitBtn.disabled = true;
    try {
      if (editing) {
        var patch = {
          name: $('pName').value.trim(), status: radioValue('pStatus'),
          brand_id: $('pBrand').value || null,
          // Explicit null clears the column (`Some(None)` server-side), which is
          // what an admin selecting "Uncategorised" means — omitting the key
          // would leave the old category in place.
          category_id: ($('pCategory') && $('pCategory').value) || null,
          style: strOrNull('pStyle'),
          price_min: parsePrice('pPriceMin'), price_max: parsePrice('pPriceMax'),
          origin: strOrNull('pOrigin'), description_md: strOrNull('pDescription'),
        };
        var res = await FerumApi.http.send('PATCH', '/api/admin/products/' + editing, patch);
        if (!res.ok) { showFormError('productFormError', await readError(res)); return; }
        // Always sent — the picker was pre-filled with the current set, so an
        // empty list is a deliberate "remove all", not "leave alone".
        await FerumApi.http.send('POST', '/api/admin/products/' + editing + '/materials', { material_ids: mats });
      } else {
        var create = {
          name: $('pName').value.trim(), slug: $('pSlug').value.trim(), product_type: radioValue('pType'),
          brand_id: $('pBrand').value || null,
          category_id: ($('pCategory') && $('pCategory').value) || null,
          style: strOrNull('pStyle'),
          price_min: parsePrice('pPriceMin'), price_max: parsePrice('pPriceMax'),
          origin: strOrNull('pOrigin'), description_md: strOrNull('pDescription'), material_ids: mats,
        };
        var cres = await FerumApi.http.send('POST', '/api/admin/products', create);
        if (!cres.ok) { showFormError('productFormError', await readError(cres)); return; }
        var created = await cres.json().catch(function () { return {}; });
        var newId = created.data && created.data.id;
        var want = radioValue('pStatus');
        if (newId && want && want !== 'draft') await FerumApi.http.send('PATCH', '/api/admin/products/' + newId, { status: want });
        // The product exists now, so the staged images finally have somewhere to
        // go. A failure here leaves the product itself created — say so rather
        // than implying the whole save failed.
        if (newId && state.pendingFiles.length) {
          var failed = await uploadPendingFiles(newId);
          if (failed) {
            closeProductModal();
            showStatus('Product created, but ' + failed + ' image(s) failed to upload. Edit the product to retry.', 'warning');
            loadProducts();
            return;
          }
        }
      }
      closeProductModal();
      showStatus(editing ? 'Product updated.' : 'Product created.', 'success');
      loadProducts();
    } catch (e) {
      showFormError('productFormError', 'Error: ' + e.message);
    } finally {
      submitBtn.disabled = false;
    }
  }

  /// Hide the dialog without tripping the unsaved-changes guard — a successful
  /// save is exactly the case where the pending edits are no longer pending.
  function closeProductModal() {
    _productDirty = false;
    modal('productModal').hide();
  }

  // The picker has to start from what the product actually has, otherwise an
  // empty picker is ambiguous — it can't tell "keep them" from "remove them all".
  // With the current set loaded, an empty picker unambiguously means "none".
  async function loadProductMaterials(productId) {
    var res = await FerumApi.http.get('/api/admin/products/' + productId + '/materials');
    if (!res.ok) return;
    var ids = (await res.json()).data || [];
    // Ignore a late response for a product the admin has already navigated away from.
    if (state.editing !== productId) return;
    state.selectedMaterials = ids;
    renderMatChips();
  }

  /// Upload every staged image in order (position follows upload order).
  /// Returns the number that failed.
  async function uploadPendingFiles(productId) {
    var failed = 0;
    for (var i = 0; i < state.pendingFiles.length; i++) {
      var fd = new FormData();
      fd.append('image', state.pendingFiles[i].file);
      var res = await FerumApi.http.postForm('/api/admin/products/' + productId + '/media', fd);
      if (!res.ok) failed++;
    }
    clearPendingFiles();
    return failed;
  }

  // ── Delete (dependency-aware) ─────────────────────────────────────────────
  function depRow(label, count, muted) {
    return '<li class="list-group-item d-flex justify-content-between px-0' +
      (muted ? ' text-muted' : '') + '"><span>' + label + '</span>' +
      '<span class="fw-semibold">' + count + '</span></li>';
  }

  async function deleteProduct(id, name) {
    state.deleting = id;
    $('delProductName').innerHTML = 'Delete <strong>' + escapeHtml(name) + '</strong>?';
    $('delDependents').innerHTML = '<li class="list-group-item px-0 text-muted">Checking…</li>';
    $('delBlocked').classList.add('d-none');
    $('delSafe').classList.add('d-none');
    $('delConfirmBtn').disabled = true;
    modal('deleteProductModal').show();

    var res = await FerumApi.http.get('/api/admin/products/' + id + '/dependents');
    if (!res.ok) {
      $('delDependents').innerHTML = '';
      $('delBlocked').textContent = await readError(res);
      $('delBlocked').classList.remove('d-none');
      return;
    }
    var d = (await res.json()).data || {};
    if (state.deleting !== id) return; // dialog moved on while we were waiting

    $('delDependents').innerHTML =
      depRow('Reviews', d.reviews, d.reviews === 0) +
      depRow('Images', d.media, d.media === 0) +
      depRow('Material links', d.materials, d.materials === 0);

    if (d.can_hard_delete) {
      $('delSafe').classList.remove('d-none');
      $('delConfirmBtn').disabled = false;
    } else {
      $('delBlocked').textContent =
        'This product has ' + d.reviews + ' review(s). Deleting it would leave them ' +
        'reviewing nothing, so permanent deletion is blocked. Archive it instead — ' +
        'it disappears from the catalogue and every review stays intact.';
      $('delBlocked').classList.remove('d-none');
      $('delConfirmBtn').disabled = true;
    }
  }

  async function confirmDeleteProduct() {
    var id = state.deleting;
    if (!id) return;
    var res = await FerumApi.http.del('/api/admin/products/' + id);
    if (!res.ok && res.status !== 204) { showStatus(await readError(res), 'danger'); return; }
    modal('deleteProductModal').hide();
    showStatus('Product deleted.', 'success');
    loadProducts();
  }

  async function archiveDeletingProduct() {
    var id = state.deleting;
    if (!id) return;
    var res = await FerumApi.http.send('PATCH', '/api/admin/products/' + id, { status: 'archived' });
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    modal('deleteProductModal').hide();
    showStatus('Product archived — its reviews were kept.', 'success');
    loadProducts();
  }

  // ── Product media ─────────────────────────────────────────────────────────
  async function loadMedia(productId) {
    var g = $('pGallery');
    g.innerHTML = '<span class="text-muted small">Loading images…</span>';
    var res = await FerumApi.http.get('/api/admin/products/' + productId + '/media');
    if (!res.ok) { g.innerHTML = ''; return; }
    var items = (await res.json()).data || [];
    if (!items.length) { g.innerHTML = '<span class="text-muted small">No images yet.</span>'; return; }
    g.innerHTML = items.map(function (md) {
      return '<div class="fr-thumb">' +
        '<img src="' + escapeHtml(md.url) + '" alt="">' +
        '<button type="button" class="fr-thumb-remove" data-media="' + md.id + '" ' +
        'title="Remove image" aria-label="Remove image">&times;</button></div>';
    }).join('');
    g.querySelectorAll('[data-media]').forEach(function (b) { b.addEventListener('click', function () { deleteMedia(productId, b.getAttribute('data-media')); }); });
  }
  async function uploadMedia(productId, fileEl) {
    var files = Array.prototype.slice.call(fileEl.files || []);
    if (!files.length) return;
    fileEl.value = '';
    for (var i = 0; i < files.length; i++) {
      var fd = new FormData(); fd.append('image', files[i]);
      var res = await FerumApi.http.postForm('/api/admin/products/' + productId + '/media', fd);
      if (!res.ok) { showStatus(await readError(res), 'danger'); break; }
    }
    loadMedia(productId); loadProducts();
  }

  // ── Staged images (create only) ───────────────────────────────────────────
  function stagePendingFiles(fileEl) {
    var files = Array.prototype.slice.call(fileEl.files || []);
    fileEl.value = '';
    files.forEach(function (f) {
      state.pendingFiles.push({ file: f, url: URL.createObjectURL(f) });
    });
    renderPendingGallery();
  }

  function renderPendingGallery() {
    var hint = $('pImagePending');
    var g = $('pGallery');
    if (!state.pendingFiles.length) {
      hint.classList.add('d-none');
      if (!state.editing) g.innerHTML = '';
      return;
    }
    hint.classList.remove('d-none');
    g.innerHTML = state.pendingFiles.map(function (p, i) {
      return '<div class="fr-thumb">' +
        '<img src="' + p.url + '" alt="">' +
        '<button type="button" class="fr-thumb-remove" data-pending="' + i + '" ' +
        'title="Remove image" aria-label="Remove image">&times;</button></div>';
    }).join('');
    g.querySelectorAll('[data-pending]').forEach(function (b) {
      b.addEventListener('click', function () {
        var i = parseInt(b.getAttribute('data-pending'), 10);
        URL.revokeObjectURL(state.pendingFiles[i].url);
        state.pendingFiles.splice(i, 1);
        renderPendingGallery();
      });
    });
  }
  async function deleteMedia(productId, mediaId) {
    var res = await FerumApi.http.del('/api/admin/products/' + productId + '/media/' + mediaId);
    if (!res.ok && res.status !== 204) { showStatus(await readError(res), 'danger'); return; }
    loadMedia(productId);
  }

  // ── Materials tab ─────────────────────────────────────────────────────────
  var MAT_CATS = {
    wood_natural: 'Solid wood', wood_engineered: 'Engineered wood', rattan_bamboo: 'Rattan & bamboo',
    metal: 'Metal', fabric: 'Fabric', leather: 'Leather', stone: 'Stone', glass: 'Glass', plastic: 'Plastic', other: 'Other',
  };
  function renderMaterialRows(list) {
    list = list || state.materials;
    var tbody = $('materialRows');
    if (!tbody) return;
    if (!list.length) {
      tbody.innerHTML = '<tr><td colspan="3" class="text-center text-muted py-4">' +
        (state.materials.length ? 'No materials match this filter.' : 'No materials yet.') + '</td></tr>';
      return;
    }
    tbody.innerHTML = list.map(function (m) {
      return '<tr><td><div class="fw-semibold">' + escapeHtml(m.name) + '</div><div class="text-muted small">/' + escapeHtml(m.slug) + '</div></td>' +
        '<td class="small">' + escapeHtml(MAT_CATS[m.category] || m.category) + '</td>' +
        '<td class="text-end"><button class="btn btn-sm btn-outline-secondary me-1" data-medit="' + m.id + '"><i class="fa-solid fa-pen"></i></button>' +
        '<button class="btn btn-sm btn-outline-danger" data-mdel="' + m.id + '" data-name="' + escapeHtml(m.name) + '"><i class="fa-solid fa-trash"></i></button></td></tr>';
    }).join('');
    tbody.querySelectorAll('[data-medit]').forEach(function (b) { b.addEventListener('click', function () { var m = state.materials.find(function (x) { return x.id === b.getAttribute('data-medit'); }); if (m) openMaterial(m); }); });
    tbody.querySelectorAll('[data-mdel]').forEach(function (b) { b.addEventListener('click', function () { deleteMaterial(b.getAttribute('data-mdel'), b.getAttribute('data-name')); }); });
  }
  async function loadMaterials() {
    var res = await FerumApi.http.get('/api/materials');
    if (!res.ok) return;
    state.materials = (await res.json()).data || [];
    applyMaterialFilter();
  }
  function applyMaterialFilter() {
    var qEl = $('matFilterQuery'), cEl = $('matFilterCategory');
    var q = (qEl ? qEl.value : '').trim().toLowerCase();
    var cat = cEl ? cEl.value : '';
    renderMaterialRows(state.materials.filter(function (m) {
      var okq = !q || m.name.toLowerCase().indexOf(q) !== -1 || (m.slug && m.slug.toLowerCase().indexOf(q) !== -1);
      return okq && (!cat || m.category === cat);
    }));
  }
  function openMaterial(m) {
    clearErrors('materialForm', 'materialFormError');
    $('materialModalTitle').textContent = m ? 'Edit material' : 'New material';
    $('mId').value = m ? m.id : '';
    $('mName').value = m ? m.name : '';
    $('mSlug').value = m ? m.slug : '';
    $('mSlug').readOnly = !!m;
    $('mCategory').value = m ? m.category : 'wood_natural';
    $('mDescription').value = m && m.description ? m.description : '';
    modal('materialModal').show();
  }
  async function submitMaterial(ev) {
    ev.preventDefault();
    clearErrors('materialForm', 'materialFormError');
    var id = $('mId').value;
    if (!$('mName').value.trim()) setFieldError('mName', 'mNameFeedback', 'Enter a material name.');
    // Slug is set once at creation; on edit it is read-only and not resubmitted.
    if (!id) {
      var mslug = $('mSlug').value.trim();
      if (!mslug) setFieldError('mSlug', 'mSlugFeedback', 'A URL slug is required.');
      else if (!/^[a-z0-9-]+$/.test(mslug)) setFieldError('mSlug', 'mSlugFeedback', 'Use lowercase letters, numbers and hyphens only.');
    }
    if (focusFirstInvalid('materialForm')) return;

    var body = { name: $('mName').value.trim(), category: $('mCategory').value, description: strOrNull('mDescription') };
    var res = id
      ? await FerumApi.http.send('PATCH', '/api/admin/materials/' + id, body)
      : await FerumApi.http.send('POST', '/api/admin/materials', { name: body.name, slug: $('mSlug').value.trim(), category: body.category, description: body.description });
    if (!res.ok) { showFormError('materialFormError', await readError(res)); return; }
    modal('materialModal').hide();
    showStatus(id ? 'Material updated.' : 'Material created.', 'success');
    loadMaterials();
  }
  async function deleteMaterial(id, name) {
    if (!confirm('Delete material "' + name + '"? Products will be unlinked from it.')) return;
    var res = await FerumApi.http.del('/api/admin/materials/' + id);
    if (!res.ok && res.status !== 204) { showStatus(await readError(res), 'danger'); return; }
    showStatus('Material deleted.', 'success'); loadMaterials();
  }

  // ── Brands tab ────────────────────────────────────────────────────────────
  function renderBrandRows(list) {
    list = list || state.brands;
    var tbody = $('brandRows');
    if (!tbody) return;
    if (!list.length) {
      tbody.innerHTML = '<tr><td colspan="4" class="text-center text-muted py-4">' +
        (state.brands.length ? 'No brands match this filter.' : 'No brands yet.') + '</td></tr>';
      return;
    }
    tbody.innerHTML = list.map(function (b) {
      return '<tr><td><div class="fw-semibold">' + escapeHtml(b.name) + '</div><div class="text-muted small">/' + escapeHtml(b.slug) + '</div></td>' +
        '<td class="small">' + escapeHtml(b.country || '—') + '</td>' +
        '<td>' + (b.is_verified ? '<i class="fa-solid fa-circle-check text-primary"></i>' : '<span class="text-muted">—</span>') + '</td>' +
        '<td class="text-end"><button class="btn btn-sm btn-outline-secondary me-1" data-bedit="' + b.id + '"><i class="fa-solid fa-pen"></i></button>' +
        '<button class="btn btn-sm btn-outline-danger" data-bdel="' + b.id + '" data-name="' + escapeHtml(b.name) + '"><i class="fa-solid fa-trash"></i></button></td></tr>';
    }).join('');
    tbody.querySelectorAll('[data-bedit]').forEach(function (x) { x.addEventListener('click', function () { var b = state.brands.find(function (y) { return y.id === x.getAttribute('data-bedit'); }); if (b) openBrand(b); }); });
    tbody.querySelectorAll('[data-bdel]').forEach(function (x) { x.addEventListener('click', function () { deleteBrand(x.getAttribute('data-bdel'), x.getAttribute('data-name')); }); });
  }
  async function loadBrands() {
    var res = await FerumApi.http.get('/api/brands');
    if (!res.ok) return;
    state.brands = (await res.json()).data || [];
    var sel = $('pBrand');
    if (sel) sel.innerHTML = '<option value="">— None —</option>' + state.brands.map(function (b) { return '<option value="' + b.id + '">' + escapeHtml(b.name) + '</option>'; }).join('');
    applyBrandFilter();
  }
  function applyBrandFilter() {
    var qEl = $('brandFilterQuery'), vEl = $('brandFilterVerified');
    var q = (qEl ? qEl.value : '').trim().toLowerCase();
    var v = vEl ? vEl.value : '';
    renderBrandRows(state.brands.filter(function (b) {
      var okq = !q || b.name.toLowerCase().indexOf(q) !== -1 || (b.country && b.country.toLowerCase().indexOf(q) !== -1);
      var okv = v === '' || (v === '1' ? !!b.is_verified : !b.is_verified);
      return okq && okv;
    }));
  }
  function openBrand(b) {
    clearErrors('brandForm', 'brandFormError');
    $('brandModalTitle').textContent = b ? 'Edit brand' : 'New brand';
    $('bId').value = b ? b.id : '';
    $('bName').value = b ? b.name : '';
    $('bSlug').value = b ? b.slug : '';
    $('bSlug').readOnly = !!b;
    $('bWebsite').value = b && b.website ? b.website : '';
    $('bCountry').value = b && b.country ? b.country : '';
    $('bDescription').value = b && b.description ? b.description : '';
    $('bVerified').checked = !!(b && b.is_verified);
    modal('brandModal').show();
  }
  async function submitBrand(ev) {
    ev.preventDefault();
    clearErrors('brandForm', 'brandFormError');
    var id = $('bId').value;
    if (!$('bName').value.trim()) setFieldError('bName', 'bNameFeedback', 'Enter a brand name.');
    if (!id) {
      var bslug = $('bSlug').value.trim();
      if (!bslug) setFieldError('bSlug', 'bSlugFeedback', 'A URL slug is required.');
      else if (!/^[a-z0-9-]+$/.test(bslug)) setFieldError('bSlug', 'bSlugFeedback', 'Use lowercase letters, numbers and hyphens only.');
    }
    var site = $('bWebsite').value.trim();
    if (site && !/^https?:\/\/.+/i.test(site)) {
      setFieldError('bWebsite', 'bWebsiteFeedback', 'Enter a full URL starting with http:// or https://');
    }
    if (focusFirstInvalid('brandForm')) return;

    var body = { name: $('bName').value.trim(), website: strOrNull('bWebsite'), country: strOrNull('bCountry'), description: strOrNull('bDescription'), is_verified: $('bVerified').checked };
    var res = id
      ? await FerumApi.http.send('PATCH', '/api/admin/brands/' + id, body)
      : await FerumApi.http.send('POST', '/api/admin/brands', { name: body.name, slug: $('bSlug').value.trim(), website: body.website, country: body.country, description: body.description, is_verified: body.is_verified });
    if (!res.ok) { showFormError('brandFormError', await readError(res)); return; }
    modal('brandModal').hide();
    showStatus(id ? 'Brand updated.' : 'Brand created.', 'success');
    loadBrands();
  }
  async function deleteBrand(id, name) {
    if (!confirm('Delete brand "' + name + '"? Products will be unlinked from it.')) return;
    var res = await FerumApi.http.del('/api/admin/brands/' + id);
    if (!res.ok && res.status !== 204) { showStatus(await readError(res), 'danger'); return; }
    showStatus('Brand deleted.', 'success'); loadBrands();
  }

  // ── Wire up ───────────────────────────────────────────────────────────────
  document.addEventListener('DOMContentLoaded', function () {
    initTabs();
    initMatPicker();

    // Auto-slug from name (create only), until admin edits slug manually.
    $('pName').addEventListener('input', function () {
      if (!state.editing && !_slugManual) {
        $('pSlug').value = slugify($('pName').value);
        renderSlugPreview();
      }
    });
    // The slug field is a disclosure, not a permanently visible control: it opens
    // on demand and stays open once the admin has taken manual control of it.
    $('pSlugEdit').addEventListener('click', function () {
      _slugManual = true;
      $('pSlugEditWrap').classList.remove('d-none');
      $('pSlug').focus();
      $('pSlug').select();
    });
    $('pSlug').addEventListener('input', renderSlugPreview);

    // Re-group the digits once the field loses focus, so a saved product reads
    // back the same way the catalogue renders it.
    ['pPriceMin', 'pPriceMax'].forEach(function (id) {
      $(id).addEventListener('blur', function () { formatPriceField($(id)); });
    });

    // Clearing a field's error as soon as it is touched keeps the red state from
    // outliving the problem it described.
    $('productForm').addEventListener('input', function (e) {
      if (e.target.classList && e.target.classList.contains('is-invalid')) {
        e.target.classList.remove('is-invalid');
      }
      _productDirty = true;
    });

    $('mName').addEventListener('input', function () { if (!$('mId').value) $('mSlug').value = slugify($('mName').value); });
    $('bName').addEventListener('input', function () { if (!$('bId').value) $('bSlug').value = slugify($('bName').value); });

    $('btnNewProduct').addEventListener('click', resetProductForm);
    $('productForm').addEventListener('submit', submitProduct);

    // Guard against losing a half-filled form to a stray backdrop click or Esc.
    // `hide.bs.modal` is cancellable; `hidden` is not, so the check has to run here.
    $('productModal').addEventListener('hide.bs.modal', function (e) {
      if (!_productDirty) return;
      if (!confirm('Discard your unsaved changes to this product?')) e.preventDefault();
      else _productDirty = false;
    });
    $('productModal').addEventListener('shown.bs.modal', function () {
      _productDirty = false;
      $('pName').focus();
    });
    // Editing uploads straight away; creating stages until the product exists.
    $('pImageInput').addEventListener('change', function () {
      if (state.editing) uploadMedia(state.editing, $('pImageInput'));
      else stagePendingFiles($('pImageInput'));
    });
    $('delConfirmBtn').addEventListener('click', confirmDeleteProduct);
    $('delArchiveBtn').addEventListener('click', archiveDeletingProduct);

    $('btnNewMaterial').addEventListener('click', function () { openMaterial(null); });
    $('materialForm').addEventListener('submit', submitMaterial);
    $('matApply').addEventListener('click', applyMaterialFilter);
    $('matFilterQuery').addEventListener('input', applyMaterialFilter);
    $('matFilterCategory').addEventListener('change', applyMaterialFilter);

    $('btnNewBrand').addEventListener('click', function () { openBrand(null); });
    $('brandForm').addEventListener('submit', submitBrand);
    $('brandApply').addEventListener('click', applyBrandFilter);
    $('brandFilterQuery').addEventListener('input', applyBrandFilter);
    $('brandFilterVerified').addEventListener('change', applyBrandFilter);

    $('applyFilters').addEventListener('click', function () { state.page = 1; loadProducts(); });
    $('filterQuery').addEventListener('keydown', function (e) { if (e.key === 'Enter') { state.page = 1; loadProducts(); } });
    $('catalogPager').addEventListener('click', function (e) {
      var btn = e.target.closest('[data-page]');
      if (!btn) return;
      var p = parseInt(btn.getAttribute('data-page'), 10);
      if (p && p !== state.page) { state.page = p; loadProducts(); }
    });

    // Reference data feeds the product modal too — load all up front. Use
    // allSettled so a failure in one loader never blocks the others (the product
    // table must render even if materials/brands can't load).
    $('btnNewPcat').addEventListener('click', function () { openPcat(null); });
    $('pcatForm').addEventListener('submit', submitPcat);
    $('btnAutoAssignPreview').addEventListener('click', function () { autoAssign(false); });
    $('btnAutoAssignApply').addEventListener('click', function () { autoAssign(true); });

    // Categories load with the other reference data: the product table renders
    // a category column, so it cannot draw correctly before they arrive.
    Promise.allSettled([loadBrands(), loadMaterials(), loadPcats()])
      .then(loadProducts)
      .then(openDeepLinkedProduct);
  });

  // Deep link into a product's full record: /admin/products?edit=<id>. Everyday
  // corrections now happen on the product page itself; this remains the way into
  // the fields that page deliberately omits (slug, status).
  async function openDeepLinkedProduct() {
    var id = new URLSearchParams(window.location.search).get('edit');
    if (!id) return;
    var res = await FerumApi.http.get('/api/admin/products/' + encodeURIComponent(id));
    if (res.ok) {
      openProduct((await res.json()).data);
    } else {
      showStatus(await readError(res), 'warning');
    }
    // Drop the parameter so a refresh doesn't reopen the modal.
    window.history.replaceState({}, '', window.location.pathname);
  }

  // ── Product categories ───────────────────────────────────────────────────
  // The catalogue's own taxonomy. The list is loaded rather than read off the
  // server-rendered <option>s so that creating a category updates every picker
  // on the page without a reload.

  async function loadPcats() {
    var res = await FerumApi.http.get('/api/admin/product-categories');
    if (!res.ok) return;
    var body = await res.json();
    state.pcats = body.data || [];
    state.unfiled = (body.meta && body.meta.unfiled) || 0;
    renderPcatRows();
    refreshCategoryPickers();
    renderUnfiledNotice();
  }

  // Both the product form's picker and the list filter are rebuilt from the
  // same array — two hand-maintained option lists would drift the first time
  // someone renamed a category.
  function refreshCategoryPickers() {
    var opts = state.pcats.map(function (c) {
      return '<option value="' + c.id + '">' + (c.parent_id ? '— ' : '') + escapeHtml(c.name) + '</option>';
    }).join('');

    var form = $('pCategory');
    if (form) {
      var keep = form.value;
      var blank = form.querySelector('option[value=""]');
      form.innerHTML = '<option value="">' + escapeHtml(blank ? blank.textContent.trim() : 'Uncategorised') + '</option>' + opts;
      form.value = keep;
    }
    var filter = $('filterCategory');
    if (filter) {
      var keepF = filter.value;
      var first = filter.querySelector('option[value=""]');
      var none = filter.querySelector('option[value="none"]');
      filter.innerHTML =
        '<option value="">' + escapeHtml(first ? first.textContent.trim() : 'All categories') + '</option>' +
        '<option value="none">' + escapeHtml(none ? none.textContent.trim() : 'Uncategorised') + '</option>' + opts;
      filter.value = keepF;
    }
  }

  function renderUnfiledNotice() {
    var el = $('pcatUnfiled');
    if (!el) return;
    if (!state.unfiled) { el.textContent = ''; return; }
    el.className = 'small mb-0 mt-2 text-warning-emphasis';
    el.textContent = state.unfiled + ' products have no category';
  }

  function renderPcatRows() {
    var tbody = $('pcatRows');
    if (!tbody) return;
    if (!state.pcats.length) {
      tbody.innerHTML = '<tr><td colspan="4" class="text-center text-muted py-4">No categories yet.</td></tr>';
      return;
    }
    tbody.innerHTML = state.pcats.map(function (c) {
      var kw = (c.match_keywords || []).join(', ');
      return '<tr>' +
        '<td><div class="fw-semibold">' + (c.icon ? '<i class="fa-solid ' + escapeHtml(c.icon) + ' me-2 text-body-secondary"></i>' : '') +
          escapeHtml(c.name) + '</div><div class="text-muted small">/' + escapeHtml(c.slug) + '</div></td>' +
        '<td class="small text-body-secondary">' + (kw ? escapeHtml(kw) : '<span class="text-muted">—</span>') + '</td>' +
        '<td class="small">' + c.product_count + '</td>' +
        '<td class="text-end">' +
          '<button class="btn btn-sm btn-outline-secondary me-1" data-cedit="' + c.id + '"><i class="fa-solid fa-pen"></i></button>' +
          '<button class="btn btn-sm btn-outline-danger" data-cdel="' + c.id + '" data-name="' + escapeHtml(c.name) + '"><i class="fa-solid fa-trash"></i></button>' +
        '</td></tr>';
    }).join('');

    tbody.querySelectorAll('[data-cedit]').forEach(function (b) {
      b.addEventListener('click', function () {
        var c = state.pcats.find(function (x) { return x.id === b.getAttribute('data-cedit'); });
        if (c) openPcat(c);
      });
    });
    tbody.querySelectorAll('[data-cdel]').forEach(function (b) {
      b.addEventListener('click', function () { deletePcat(b.getAttribute('data-cdel'), b.getAttribute('data-name')); });
    });
  }

  function openPcat(c) {
    clearErrors('pcatForm', 'pcatFormError');
    $('pcatModalTitle').textContent = c ? 'Edit category' : 'New category';
    $('cId').value = c ? c.id : '';
    $('cName').value = c ? c.name : '';
    $('cSlug').value = c ? c.slug : '';
    // Slug is a stable key: shown on edit, never editable.
    $('cSlug').disabled = !!c;
    $('cKeywords').value = c ? (c.match_keywords || []).join(', ') : '';
    $('cIcon').value = c ? (c.icon || '') : '';
    $('cPosition').value = c ? c.position : 0;
    modal('pcatModal').show();
  }

  function keywordList() {
    return $('cKeywords').value.split(',').map(function (s) { return s.trim(); }).filter(Boolean);
  }

  async function submitPcat(e) {
    e.preventDefault();
    clearErrors('pcatForm', 'pcatFormError');
    var id = $('cId').value;
    var btn = $('pcatSubmit');
    btn.disabled = true;
    try {
      var res;
      if (id) {
        res = await FerumApi.http.send('PATCH', '/api/admin/product-categories/' + id, {
          name: $('cName').value.trim(),
          icon: $('cIcon').value.trim() || null,
          position: parseInt($('cPosition').value, 10) || 0,
          match_keywords: keywordList(),
        });
      } else {
        res = await FerumApi.http.send('POST', '/api/admin/product-categories', {
          name: $('cName').value.trim(),
          slug: $('cSlug').value.trim(),
          icon: $('cIcon').value.trim() || null,
          position: parseInt($('cPosition').value, 10) || 0,
          match_keywords: keywordList(),
        });
      }
      if (!res.ok) { showFormError('pcatFormError', await readError(res)); return; }
      modal('pcatModal').hide();
      showStatus('Category saved.', 'success');
      await loadPcats();
      loadProducts();
    } catch (err) {
      showFormError('pcatFormError', 'Error: ' + err.message);
    } finally {
      btn.disabled = false;
    }
  }

  async function deletePcat(id, name) {
    // Products are not deleted with the category — they fall back to unfiled —
    // so the confirmation says so rather than implying data loss.
    if (!confirm('Delete "' + name + '"? Products filed here become uncategorised.')) return;
    var res = await FerumApi.http.del('/api/admin/product-categories/' + id);
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    showStatus('Category deleted.', 'success');
    await loadPcats();
    loadProducts();
  }

  // Preview first, apply second. A bulk write across the whole catalogue is not
  // something to trigger from a single click with no idea of the blast radius.
  async function autoAssign(apply) {
    var url = '/api/admin/product-categories/auto-assign' + (apply ? '?apply=1' : '');
    var res = await FerumApi.http.post(url);
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    var report = (await res.json()).data || {};

    if (apply) {
      $('pcatPreviewWrap').classList.add('d-none');
      showStatus(report.assigned + ' filed · ' + report.unmatched + ' still unfiled', 'success');
      await loadPcats();
      loadProducts();
      return;
    }

    $('pcatPreview').textContent = report.assigned + ' would be filed · ' + report.unmatched + ' left unfiled';
    // Nothing to apply is not a call to action — say so and leave the button away.
    $('pcatPreviewWrap').classList.toggle('d-none', !report.assigned);
    if (!report.assigned) showStatus('Nothing left to file automatically.', 'info');
  }
}());
