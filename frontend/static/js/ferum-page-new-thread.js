(function () {
  'use strict';

  var _thumbState = null;

  window.newThreadThumbnail = function () {
    var state = {
      file: null,
      preview: null,
      thumbError: '',
      isDragging: false,
      onDrop: function (e) { var f = e.dataTransfer.files && e.dataTransfer.files[0]; if (f) this.handleFile(f); },
      onFileChange: function (e) { var f = e.target.files && e.target.files[0]; if (f) this.handleFile(f); },
      handleFile: function (f) {
        var ACCEPTED = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];
        this.thumbError = '';
        if (!ACCEPTED.includes(f.type)) { this.thumbError = 'Thumbnail must be a JPEG, PNG, GIF, or WebP image.'; return; }
        if (f.size > 10 * 1024 * 1024) { this.thumbError = 'Thumbnail must be under 10 MB.'; return; }
        this.file = f;
        var reader = new FileReader();
        var self = this;
        reader.onload = function (ev) { self.preview = ev.target.result; };
        reader.readAsDataURL(f);
      },
      remove: function () { this.file = null; this.preview = null; this.thumbError = ''; },
      uploadTo: async function (threadId) {
        if (!this.file) return;
        var fd = new FormData();
        fd.append('thumbnail', this.file);
        await FerumApi.threads.uploadThumbnail(threadId, fd);
      },
    };
    _thumbState = state;
    return state;
  };

  // tagChipInput used only on this page (max 5 tags, no initial seed)
  window.tagChipInput = function () {
    return {
      tags: [],
      current: '',
      add: function () {
        var t = this.current.trim().replace(/,+$/, '');
        if (t && !this.tags.includes(t) && this.tags.length < 5) this.tags.push(t);
        this.current = '';
      },
      checkComma: function (e) {
        if (e.key === ',') { e.preventDefault(); this.add(); }
      },
      remove: function (idx) {
        if (idx >= 0 && idx < this.tags.length) this.tags.splice(idx, 1);
      },
    };
  };

  // ── Product review: searchable typeahead + rating block orchestration ──
  var _ratingDims = ['overall', 'durability', 'materials', 'comfort', 'aesthetics', 'value_for_money'];
  var _productSearchTimer = null;
  var _typeLabels = { furniture: 'Furniture', material: 'Material', room: 'Room' };
  // Mirrors the material_category_label macro in macros.html.
  var _matCatLabels = {
    wood_natural: 'Solid wood', wood_engineered: 'Engineered wood', rattan_bamboo: 'Rattan & bamboo',
    metal: 'Metal', fabric: 'Fabric', leather: 'Leather', stone: 'Stone', glass: 'Glass',
    plastic: 'Plastic', other: 'Other',
  };
  // Set by initProductReview so the "add product" flow can select what it created.
  var _selectProduct = null;

  function initProductReview() {
    var field = document.getElementById('product-field');
    if (!field) return;
    var searchWrap  = document.getElementById('product-search-wrap');
    var searchInput = document.getElementById('product-search');
    var resultsEl   = document.getElementById('product-results');
    var hiddenId    = document.getElementById('product-id');
    var selectedBox = document.getElementById('product-selected');
    var selectedName = document.getElementById('product-selected-name');
    var clearBtn    = document.getElementById('product-clear');
    var ratingBlock = document.getElementById('rating-block');

    var addBlock = document.getElementById('product-add');
    function setAddVisible(v) { if (addBlock) addBlock.classList.toggle('d-none', !v); }
    function showResults(show) {
      resultsEl.classList.toggle('d-none', !show);
      if (!show) setAddVisible(true); // dropdown closed → restore the "add" affordance
    }

    // Reviews are auto-filed into the server-side "Reviews" category, so the
    // forum-category picker is irrelevant (and hidden) whenever a product is
    // attached. It reappears for a plain thread.
    var catField = document.getElementById('category-field');
    var catSelect = document.getElementById('category');
    function setCategoryVisible(visible) {
      if (catField) catField.classList.toggle('d-none', !visible);
      if (catSelect) {
        if (visible) catSelect.setAttribute('required', 'required');
        else catSelect.removeAttribute('required');
      }
    }

    // Reframe the page's identity: writing a product review, not a plain thread.
    var heading = document.getElementById('compose-heading');
    var submitBtn = document.getElementById('submit-btn');
    function setReviewFraming(on) {
      if (heading) heading.textContent = on ? 'Write a review' : 'New post';
      // The submit button's first child is its text node (spinner span follows).
      if (submitBtn && submitBtn.childNodes[0]) {
        submitBtn.childNodes[0].nodeValue = on ? 'Publish review' : 'Publish';
      }
    }

    function selectProduct(id, name) {
      hiddenId.value = id || '';
      selectedName.textContent = name || '';
      selectedBox.classList.remove('d-none');
      selectedBox.classList.add('d-flex');
      searchWrap.classList.add('d-none');
      showResults(false);
      setAddVisible(false); // a product is locked in — "add new" no longer relevant
      if (ratingBlock) ratingBlock.style.display = '';
      setCategoryVisible(false);
      setReviewFraming(true);
    }

    // Wipe any scores entered for the previous product so they can't be
    // silently carried over to a different one.
    function resetRatings() {
      if (!ratingBlock) return;
      Array.prototype.forEach.call(ratingBlock.querySelectorAll('.fr-star-input'), function (group) {
        var hidden = group.querySelector('input[type="hidden"]');
        if (hidden) hidden.value = '';
        Array.prototype.forEach.call(group.querySelectorAll('.fr-star-btn i'), function (i) {
          i.className = 'fa-regular fa-star';
        });
        var hint = group.parentElement && group.parentElement.querySelector('[data-rate-hint]');
        if (hint) {
          hint.textContent = hint.getAttribute('data-empty') || '';
          hint.classList.remove('text-warning', 'fw-semibold');
          hint.classList.add('text-muted');
        }
      });
      var vp = document.getElementById('rate-verified');
      if (vp) vp.checked = false;
    }

    function clearProduct() {
      hiddenId.value = '';
      selectedBox.classList.add('d-none');
      selectedBox.classList.remove('d-flex');
      searchWrap.classList.remove('d-none');
      searchInput.value = '';
      resultsEl.innerHTML = '';
      showResults(false);
      resetRatings();
      if (ratingBlock) ratingBlock.style.display = 'none';
      setCategoryVisible(true);
      setReviewFraming(false);
      searchInput.focus();
    }

    function renderResults(items) {
      resultsEl.innerHTML = '';
      if (!items.length) {
        var empty = document.createElement('div');
        empty.className = 'list-group-item text-muted small';
        empty.textContent = 'No products found.';
        resultsEl.appendChild(empty);
        setAddVisible(true); // no matches → keep "add product" reachable below
        showResults(true);
        return;
      }
      // Matches shown → hide the "add product" toggle so it can't collide with
      // the dropdown that overlays it.
      setAddVisible(false);
      items.forEach(function (p) {
        var btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'list-group-item list-group-item-action d-flex justify-content-between align-items-center';
        var left = document.createElement('span');
        left.className = 'text-truncate';
        left.textContent = p.name;
        // Your own not-yet-approved submission.
        if (p.status && p.status !== 'published') {
          var pend = document.createElement('span');
          pend.className = 'badge text-bg-warning ms-2';
          pend.textContent = 'Pending approval';
          left.appendChild(pend);
        }
        var right = document.createElement('span');
        right.className = 'badge text-bg-light ms-2 flex-shrink-0';
        right.textContent = _typeLabels[p.product_type] || p.product_type || '';
        btn.appendChild(left);
        btn.appendChild(right);
        btn.addEventListener('click', function () { selectProduct(p.id, p.name); });
        resultsEl.appendChild(btn);
      });
      showResults(true);
    }

    function runSearch() {
      var term = searchInput.value.trim();
      var url = '/api/products?per_page=20' + (term ? '&q=' + encodeURIComponent(term) : '');
      fetch(url, { headers: { Accept: 'application/json' } })
        .then(function (r) { return r.ok ? r.json() : { data: [] }; })
        .then(function (body) { renderResults(body.data || []); })
        .catch(function () { showResults(false); });
    }

    searchInput.addEventListener('input', function () {
      clearTimeout(_productSearchTimer);
      _productSearchTimer = setTimeout(runSearch, 250);
    });
    searchInput.addEventListener('focus', function () {
      if (resultsEl.children.length === 0) runSearch(); else showResults(true);
    });
    document.addEventListener('click', function (e) {
      if (!field.contains(e.target)) showResults(false);
    });
    clearBtn.addEventListener('click', clearProduct);

    // Enter inside any field here must never submit the whole thread form.
    field.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && e.target.tagName === 'INPUT') {
        e.preventDefault();
        if (e.target.id === 'product-search') runSearch();
      }
    });

    _selectProduct = selectProduct;

    // Preselect when arriving from a product page (?product=slug).
    var preId = field.getAttribute('data-preselect-id');
    if (preId) selectProduct(preId, field.getAttribute('data-preselect-name'));
  }

  // "Add a product" — crowd-sourced submission (draft, pending admin approval).
  function initProductSubmit() {
    var toggle = document.getElementById('product-add-toggle');
    if (!toggle) return; // user not eligible to submit
    var form = document.getElementById('product-add-form');
    var brandSel = document.getElementById('np-brand');
    var errEl = document.getElementById('np-error');
    var submitBtn = document.getElementById('np-submit');
    var matPicker = document.getElementById('np-mat-picker');
    var refsLoaded = false;
    var pickedMaterials = []; // material ids, in the order the user tapped them

    // Brands and materials are both small reference lists — fetch once, on the
    // first time the form is opened.
    function loadRefs() {
      if (refsLoaded) return;
      refsLoaded = true;
      fetch('/api/brands', { headers: { Accept: 'application/json' } })
        .then(function (r) { return r.ok ? r.json() : { data: [] }; })
        .then(function (body) {
          (body.data || []).forEach(function (b) {
            var o = document.createElement('option');
            o.value = b.id; o.textContent = b.name;
            brandSel.appendChild(o);
          });
        })
        .catch(function () {});
      fetch('/api/materials', { headers: { Accept: 'application/json' } })
        .then(function (r) { return r.ok ? r.json() : { data: [] }; })
        .then(function (body) { renderMatPicker(body.data || []); })
        .catch(function () { setMatMessage('Could not load the material list.'); });
    }

    function setMatMessage(text) {
      matPicker.innerHTML = '';
      var p = document.createElement('div');
      p.className = 'text-muted small';
      p.textContent = text;
      matPicker.appendChild(p);
    }

    // Every material rendered up-front as a toggle chip, grouped by category —
    // the reference table is short, so browsing beats typing here.
    function renderMatPicker(materials) {
      if (!materials.length) {
        setMatMessage('No materials in the catalog yet.');
        return;
      }
      matPicker.innerHTML = '';
      var order = [];
      var byCat = {};
      materials.forEach(function (m) {
        var c = m.category || 'other';
        if (!byCat[c]) { byCat[c] = []; order.push(c); }
        byCat[c].push(m);
      });
      order.forEach(function (cat, idx) {
        var head = document.createElement('div');
        head.className = 'text-muted small text-uppercase' + (idx ? ' mt-2' : '');
        head.textContent = _matCatLabels[cat] || cat;
        matPicker.appendChild(head);

        var row = document.createElement('div');
        row.className = 'd-flex flex-wrap gap-1 mt-1';
        byCat[cat].forEach(function (m) {
          var chip = document.createElement('button');
          chip.type = 'button';
          chip.className = 'btn btn-sm btn-outline-secondary fr-mat-chip';
          chip.textContent = m.name;
          chip.setAttribute('aria-pressed', 'false');
          chip.addEventListener('click', function () {
            var i = pickedMaterials.indexOf(m.id);
            if (i === -1) pickedMaterials.push(m.id); else pickedMaterials.splice(i, 1);
            var on = i === -1;
            chip.classList.toggle('btn-secondary', on);
            chip.classList.toggle('btn-outline-secondary', !on);
            chip.setAttribute('aria-pressed', on ? 'true' : 'false');
          });
          row.appendChild(chip);
        });
        matPicker.appendChild(row);
      });
    }

    function numOrNull(id) {
      var v = (document.getElementById(id).value || '').trim();
      return v === '' ? null : parseInt(v, 10);
    }

    toggle.addEventListener('click', function () {
      form.classList.toggle('d-none');
      if (!form.classList.contains('d-none')) {
        loadRefs();
        document.getElementById('np-name').focus();
      }
    });
    document.getElementById('np-cancel').addEventListener('click', function () {
      form.classList.add('d-none');
    });

    submitBtn.addEventListener('click', async function () {
      errEl.classList.add('d-none');
      var name = (document.getElementById('np-name').value || '').trim();
      if (name.length < 1) { errEl.textContent = 'Please enter a product name.'; errEl.classList.remove('d-none'); return; }

      var payload = {
        name: name,
        product_type: document.getElementById('np-type').value,
        brand_id: brandSel.value || null,
        price_min: numOrNull('np-price-min'),
        price_max: numOrNull('np-price-max'),
        material_ids: pickedMaterials.slice(),
      };
      submitBtn.disabled = true;
      try {
        var res = await fetch('/api/products', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        });
        var body = await res.json().catch(function () { return {}; });
        if (!res.ok) {
          errEl.textContent = (body.error && body.error.message) || 'Could not add the product.';
          errEl.classList.remove('d-none');
          submitBtn.disabled = false;
          return;
        }
        var p = body.data || {};
        form.classList.add('d-none');
        if (_selectProduct) _selectProduct(p.id, p.name);
      } catch (_) {
        errEl.textContent = 'Network error. Please try again.';
        errEl.classList.remove('d-none');
        submitBtn.disabled = false;
      }
    });

    // Deep-link from the catalog "Suggest a product" CTA:
    // /new-thread?add_product=1[&name=…] opens the add-product form straight
    // away, optionally pre-filling the name from the failed catalog search.
    var params = new URLSearchParams(window.location.search);
    if (params.get('add_product')) {
      form.classList.remove('d-none');
      loadRefs();
      var presetName = params.get('name');
      var nameEl = document.getElementById('np-name');
      if (presetName) nameEl.value = presetName;
      nameEl.focus();
    }
  }

  function selectedProductId() {
    var el = document.getElementById('product-id');
    return el && el.value ? el.value : null;
  }

  function collectRating() {
    var overallEl = document.getElementById('rate-overall');
    if (!overallEl || !overallEl.value) return null;
    var body = { overall: parseInt(overallEl.value, 10) };
    _ratingDims.slice(1).forEach(function (d) {
      var el = document.getElementById('rate-' + d);
      if (el && el.value) body[d] = parseInt(el.value, 10);
    });
    var vp = document.getElementById('rate-verified');
    body.verified_purchase = !!(vp && vp.checked);
    return body;
  }

  // Enhance each `.fr-star-input` into a click-to-rate row backed by its hidden
  // input (#rate-{dim}), which collectRating() reads. Optional dimensions clear
  // when the selected star is re-clicked; the required one (overall) never clears.
  function initStars() {
    var groups = document.querySelectorAll('.fr-star-input');
    Array.prototype.forEach.call(groups, function (group) {
      var hidden = group.querySelector('input[type="hidden"]');
      var btns = Array.prototype.slice.call(group.querySelectorAll('.fr-star-btn'));
      var required = group.getAttribute('data-required') === '1';
      // Optional live hint (only the "overall" row carries one).
      var hint = group.parentElement ? group.parentElement.querySelector('[data-rate-hint]') : null;
      var hintEmpty = hint ? (hint.getAttribute('data-empty') || '') : '';

      function paint(upTo) {
        btns.forEach(function (b) {
          var v = parseInt(b.getAttribute('data-val'), 10);
          b.querySelector('i').className = (v <= upTo ? 'fa-solid' : 'fa-regular') + ' fa-star';
        });
      }
      function current() { return parseInt(hidden.value, 10) || 0; }
      function setHint(v) {
        if (!hint) return;
        var b = v ? btns[v - 1] : null;
        hint.textContent = b ? (b.getAttribute('title') || '') : hintEmpty;
        hint.classList.toggle('text-warning', !!v);
        hint.classList.toggle('fw-semibold', !!v);
        hint.classList.toggle('text-muted', !v);
      }

      btns.forEach(function (b) {
        var v = parseInt(b.getAttribute('data-val'), 10);
        b.addEventListener('click', function () {
          if (!required && current() === v) { hidden.value = ''; paint(0); setHint(0); return; }
          hidden.value = String(v);
          paint(v);
          setHint(v);
        });
        b.addEventListener('mouseenter', function () { paint(v); setHint(v); });
      });
      group.addEventListener('mouseleave', function () { paint(current()); setHint(current()); });
      paint(current());
      setHint(current());
    });
  }

  initProductReview();
  initProductSubmit();
  initStars();

  var _newThreadSubmitting = false;

  document.getElementById('new-thread-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    if (_newThreadSubmitting) return;
    var btn     = document.getElementById('submit-btn');
    var spinner = document.getElementById('btn-spinner');
    var errorEl = document.getElementById('form-error');

    function fail(msg) {
      errorEl.textContent = msg;
      errorEl.classList.remove('d-none');
      btn.disabled = false;
      spinner.classList.add('d-none');
      _newThreadSubmitting = false;
    }

    var productId = selectedProductId();
    var rating = productId ? collectRating() : null;

    // A product is attached but no overall score → block before submitting.
    if (productId && !rating) {
      fail('Please give the product at least an Overall score.');
      return;
    }

    _newThreadSubmitting = true;
    btn.disabled = true;
    spinner.classList.remove('d-none');
    errorEl.classList.add('d-none');

    var composer   = document.getElementById('composer');
    var content_md = (composer && composer.getContent) ? (composer.getContent() || '') : (document.getElementById('content')?.value || '');

    var tags = [];
    try { tags = JSON.parse(document.getElementById('tags-json')?.value || '[]'); } catch (_) {}

    var fd = new FormData();
    // Omit category for product reviews — the server files them under the
    // canonical Reviews category. Only append a real selection for plain threads.
    var catVal = document.getElementById('category').value;
    if (!productId && catVal) fd.append('category_id', catVal);
    fd.append('title', document.getElementById('title').value);
    fd.append('content_md', content_md);
    if (tags.length > 0) fd.append('tags', tags.join(','));
    if (_thumbState && _thumbState.file) fd.append('thumbnail', _thumbState.file);

    // Product review: the rating rides along in the SAME request, so the review
    // and its rating are created as one atomic operation (server rolls the thread
    // back if the rating can't be saved) — no separate call, no retry dance.
    if (productId) {
      fd.append('product_id', productId);
      fd.append('overall', String(rating.overall));
      ['durability', 'materials', 'comfort', 'aesthetics', 'value_for_money'].forEach(function (d) {
        if (rating[d] != null) fd.append(d, String(rating[d]));
      });
      fd.append('verified_purchase', rating.verified_purchase ? 'true' : 'false');
    }

    try {
      var res = await FerumApi.threads.create(fd);
      if (!res.ok) {
        var body = await res.json().catch(function () { return {}; });
        fail((body.error && body.error.message) || 'Could not publish. Please try again.');
        return;
      }
      var data = await res.json();
      var slug = (data.data && data.data.thread && data.data.thread.slug) || '';
      window.location.href = '/forum/t/' + slug;
    } catch (_) {
      fail('Network error. Please try again.');
    }
  });
}());
