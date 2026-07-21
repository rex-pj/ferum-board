/**
 * Admin catalog — drives /admin/products (tabs: Products / Materials / Brands).
 * Reads /api/admin/products, /api/materials, /api/brands; writes via /api/admin/*.
 * Same-origin fetch satisfies the Origin-based CSRF check automatically.
 */
(function () {
  'use strict';

  var state = {
    materials: [], brands: [], selectedMaterials: [],
    page: 1, perPage: 20, total: 0, editing: null,
  };
  var vnd = new Intl.NumberFormat('vi-VN');

  function $(id) { return document.getElementById(id); }

  function escapeHtml(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

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
    try { var b = await res.json(); return (b && b.error && b.error.message) || ('Error (' + res.status + ')'); }
    catch (e) { return 'Error (' + res.status + ')'; }
  }

  function getJSON(url) { return fetch(url, { method: 'GET' }); }
  function sendJSON(method, url, body) {
    return fetch(url, {
      method: method,
      headers: { 'Content-Type': 'application/json' },
      body: body != null ? JSON.stringify(body) : undefined,
    });
  }
  function strOrNull(id) { var v = $(id).value.trim(); return v === '' ? null : v; }
  function numOrNull(id) { var v = $(id).value.trim(); return v === '' ? null : parseInt(v, 10); }
  function modal(id) { return bootstrap.Modal.getOrCreateInstance($(id)); }

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
      return vnd.format(p.price_min) + ' – ' + vnd.format(p.price_max);
    return vnd.format(p.price_min != null ? p.price_min : p.price_max);
  }
  function brandName(id) {
    if (!id) return '<span class="text-muted">—</span>';
    var b = state.brands.find(function (x) { return x.id === id; });
    return b ? escapeHtml(b.name) : '<span class="text-muted">—</span>';
  }

  function renderProducts(products) {
    var tbody = $('productRows');
    if (!tbody) return;
    if (!products.length) { tbody.innerHTML = '<tr><td colspan="5" class="text-center text-muted py-4">No products yet.</td></tr>'; return; }
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
    html += page < totalPages ? pagerItem(chevR, page + 1, { ariaLabel: 'Trang sau' })
                              : pagerItem(chevR, 0, { disabled: true });
    ul.innerHTML = html;
  }

  async function loadProducts() {
    var params = new URLSearchParams();
    params.set('page', state.page); params.set('per_page', state.perPage);
    var q = $('filterQuery').value.trim(), t = $('filterType').value, s = $('filterStatus').value;
    if (q) params.set('q', q); if (t) params.set('type', t); if (s) params.set('status', s);
    var res = await getJSON('/api/admin/products?' + params.toString());
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    var body = await res.json();
    renderProducts(body.data || []);
    state.total = body.meta ? body.meta.total : 0;
    var totalPages = Math.max(1, Math.ceil(state.total / state.perPage));
    $('catalogMeta').textContent = state.total + ' products · page ' + state.page + ' / ' + totalPages;
    renderProductsPager(totalPages);
  }

  async function setProductStatus(id, status, ok) {
    var res = await sendJSON('PATCH', '/api/admin/products/' + id, { status: status });
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
  function matResults(term) {
    var box = $('pMatResults');
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
  function resetProductForm() {
    state.editing = null;
    state.selectedMaterials = [];
    $('productModalTitle').textContent = 'New product';
    $('productId').value = '';
    ['pName', 'pSlug', 'pStyle', 'pPriceMin', 'pPriceMax', 'pOrigin', 'pDescription', 'pMatSearch'].forEach(function (id) { $(id).value = ''; });
    $('pType').value = 'furniture';
    $('pStatus').value = 'published';
    $('pBrand').value = '';
    $('pSlug').readOnly = true;
    $('pSlugEdit').classList.remove('d-none');
    _slugManual = false;
    $('pMaterialsHint').textContent = 'Type a material name, then pick it to add.';
    renderMatChips();
    $('imagesSection').style.display = 'none';
    $('pGallery').innerHTML = '';
    $('pImageInput').value = '';
  }

  var _slugManual = false;

  function openProduct(p) {
    resetProductForm();
    state.editing = p.id;
    $('productModalTitle').textContent = 'Edit product';
    $('productId').value = p.id;
    $('pName').value = p.name || '';
    $('pSlug').value = p.slug || '';
    $('pSlug').readOnly = true;
    $('pSlugEdit').classList.add('d-none'); // slug immutable on edit
    $('pType').value = p.product_type || 'furniture';
    $('pStatus').value = p.status || 'draft';
    $('pBrand').value = p.brand_id || '';
    $('pStyle').value = p.style || '';
    $('pPriceMin').value = p.price_min != null ? p.price_min : '';
    $('pPriceMax').value = p.price_max != null ? p.price_max : '';
    $('pOrigin').value = p.origin || '';
    $('pDescription').value = p.description_md || '';
    $('pMaterialsHint').textContent = 'Leave empty to keep the current materials.';
    $('imagesSection').style.display = '';
    loadMedia(p.id);
    modal('productModal').show();
  }

  async function submitProduct(ev) {
    ev.preventDefault();
    var editing = state.editing;
    var mats = state.selectedMaterials.slice();
    try {
      if (editing) {
        var patch = {
          name: $('pName').value.trim(), status: $('pStatus').value,
          brand_id: $('pBrand').value || null, style: strOrNull('pStyle'),
          price_min: numOrNull('pPriceMin'), price_max: numOrNull('pPriceMax'),
          origin: strOrNull('pOrigin'), description_md: strOrNull('pDescription'),
        };
        var res = await sendJSON('PATCH', '/api/admin/products/' + editing, patch);
        if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
        if (mats.length) await sendJSON('POST', '/api/admin/products/' + editing + '/materials', { material_ids: mats });
      } else {
        var create = {
          name: $('pName').value.trim(), slug: $('pSlug').value.trim(), product_type: $('pType').value,
          brand_id: $('pBrand').value || null, style: strOrNull('pStyle'),
          price_min: numOrNull('pPriceMin'), price_max: numOrNull('pPriceMax'),
          origin: strOrNull('pOrigin'), description_md: strOrNull('pDescription'), material_ids: mats,
        };
        var cres = await sendJSON('POST', '/api/admin/products', create);
        if (!cres.ok) { showStatus(await readError(cres), 'danger'); return; }
        var created = await cres.json().catch(function () { return {}; });
        var newId = created.data && created.data.id;
        var want = $('pStatus').value;
        if (newId && want && want !== 'draft') await sendJSON('PATCH', '/api/admin/products/' + newId, { status: want });
      }
      modal('productModal').hide();
      showStatus(editing ? 'Product updated.' : 'Product created.', 'success');
      loadProducts();
    } catch (e) { showStatus('Error: ' + e.message, 'danger'); }
  }

  async function deleteProduct(id, name) {
    if (!confirm('Delete product "' + name + '"? This cannot be undone.')) return;
    var res = await fetch('/api/admin/products/' + id, { method: 'DELETE' });
    if (!res.ok && res.status !== 204) { showStatus(await readError(res), 'danger'); return; }
    showStatus('Product deleted.', 'success'); loadProducts();
  }

  // ── Product media ─────────────────────────────────────────────────────────
  async function loadMedia(productId) {
    var g = $('pGallery');
    g.innerHTML = '<span class="text-muted small">Loading images…</span>';
    var res = await getJSON('/api/admin/products/' + productId + '/media');
    if (!res.ok) { g.innerHTML = ''; return; }
    var items = (await res.json()).data || [];
    if (!items.length) { g.innerHTML = '<span class="text-muted small">No images yet.</span>'; return; }
    g.innerHTML = items.map(function (md) {
      return '<div class="position-relative" style="width:72px;height:72px">' +
        '<img src="/files/' + escapeHtml(md.storage_key) + '" class="rounded border w-100 h-100" style="object-fit:cover">' +
        '<button type="button" class="btn btn-sm btn-danger position-absolute top-0 end-0 p-0 lh-1" style="width:18px;height:18px" data-media="' + md.id + '">&times;</button></div>';
    }).join('');
    g.querySelectorAll('[data-media]').forEach(function (b) { b.addEventListener('click', function () { deleteMedia(productId, b.getAttribute('data-media')); }); });
  }
  async function uploadMedia(productId, fileEl) {
    var f = fileEl.files && fileEl.files[0]; if (!f) return;
    var fd = new FormData(); fd.append('image', f);
    var res = await fetch('/api/admin/products/' + productId + '/media', { method: 'POST', body: fd });
    fileEl.value = '';
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    loadMedia(productId); loadProducts();
  }
  async function deleteMedia(productId, mediaId) {
    var res = await fetch('/api/admin/products/' + productId + '/media/' + mediaId, { method: 'DELETE' });
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
    var res = await getJSON('/api/materials');
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
    var id = $('mId').value;
    var body = { name: $('mName').value.trim(), category: $('mCategory').value, description: strOrNull('mDescription') };
    var res = id
      ? await sendJSON('PATCH', '/api/admin/materials/' + id, body)
      : await sendJSON('POST', '/api/admin/materials', { name: body.name, slug: $('mSlug').value.trim(), category: body.category, description: body.description });
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    modal('materialModal').hide();
    showStatus(id ? 'Material updated.' : 'Material created.', 'success');
    loadMaterials();
  }
  async function deleteMaterial(id, name) {
    if (!confirm('Delete material "' + name + '"? Products will be unlinked from it.')) return;
    var res = await fetch('/api/admin/materials/' + id, { method: 'DELETE' });
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
    var res = await getJSON('/api/brands');
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
    var id = $('bId').value;
    var body = { name: $('bName').value.trim(), website: strOrNull('bWebsite'), country: strOrNull('bCountry'), description: strOrNull('bDescription'), is_verified: $('bVerified').checked };
    var res = id
      ? await sendJSON('PATCH', '/api/admin/brands/' + id, body)
      : await sendJSON('POST', '/api/admin/brands', { name: body.name, slug: $('bSlug').value.trim(), website: body.website, country: body.country, description: body.description });
    if (!res.ok) { showStatus(await readError(res), 'danger'); return; }
    modal('brandModal').hide();
    showStatus(id ? 'Brand updated.' : 'Brand created.', 'success');
    loadBrands();
  }
  async function deleteBrand(id, name) {
    if (!confirm('Delete brand "' + name + '"? Products will be unlinked from it.')) return;
    var res = await fetch('/api/admin/brands/' + id, { method: 'DELETE' });
    if (!res.ok && res.status !== 204) { showStatus(await readError(res), 'danger'); return; }
    showStatus('Brand deleted.', 'success'); loadBrands();
  }

  // ── Wire up ───────────────────────────────────────────────────────────────
  document.addEventListener('DOMContentLoaded', function () {
    initTabs();
    initMatPicker();

    // Auto-slug from name (create only), until admin edits slug manually.
    $('pName').addEventListener('input', function () {
      if (!state.editing && !_slugManual) $('pSlug').value = slugify($('pName').value);
    });
    $('pSlugEdit').addEventListener('click', function () {
      _slugManual = true; $('pSlug').readOnly = false; $('pSlug').focus();
    });
    $('mName').addEventListener('input', function () { if (!$('mId').value) $('mSlug').value = slugify($('mName').value); });
    $('bName').addEventListener('input', function () { if (!$('bId').value) $('bSlug').value = slugify($('bName').value); });

    $('btnNewProduct').addEventListener('click', resetProductForm);
    $('productForm').addEventListener('submit', submitProduct);
    $('pImageInput').addEventListener('change', function () { if (state.editing) uploadMedia(state.editing, $('pImageInput')); });

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
    Promise.allSettled([loadBrands(), loadMaterials()]).then(loadProducts);
  });
}());
