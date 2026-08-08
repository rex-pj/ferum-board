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
        if (!ACCEPTED.includes(f.type)) { this.thumbError = Ferum.t('js-thumbnail-invalid-type'); return; }
        if (f.size > 10 * 1024 * 1024) { this.thumbError = Ferum.t('js-thumbnail-too-large'); return; }
        this.file = f;
        var reader = new FileReader();
        var self = this;
        reader.onload = function (ev) { self.preview = ev.target.result; };
        reader.readAsDataURL(f);
      },
      remove: function () { this.file = null; this.preview = null; this.thumbError = ''; },
      uploadTo: async function (threadSlug) {
        if (!this.file) return;
        var fd = new FormData();
        // 'file': POST /api/threads/{slug}/thumbnail reads that part name only.
        // (The multipart POST /api/threads create path uses 'thumbnail' instead.)
        fd.append('file', this.file);
        await FerumApi.threads.uploadThumbnail(threadSlug, fd);
      },
    };
    _thumbState = state;
    return state;
  };

  // tagChipInput comes from ferum-api.js — one implementation, one tag limit.
  // The copy that used to live here was identical bar the seeding it did not do.

  // ── Product review: searchable typeahead + rating block orchestration ──
  var _ratingDims = ['overall', 'durability', 'materials', 'comfort', 'aesthetics', 'value_for_money'];
  var _productSearchTimer = null;
  var _typeLabels = {
    furniture: Ferum.t('js-product-type-furniture'),
    material: Ferum.t('js-product-type-material'),
    room: Ferum.t('js-product-type-room'),
  };
  // Mirrors the material_category_label macro in macros.html.
  var _matCatLabels = {
    wood_natural: Ferum.t('js-material-wood-natural'),
    wood_engineered: Ferum.t('js-material-wood-engineered'),
    rattan_bamboo: Ferum.t('js-material-rattan-bamboo'),
    metal: Ferum.t('js-material-metal'),
    fabric: Ferum.t('js-material-fabric'),
    leather: Ferum.t('js-material-leather'),
    stone: Ferum.t('js-material-stone'),
    glass: Ferum.t('js-material-glass'),
    plastic: Ferum.t('js-material-plastic'),
    other: Ferum.t('js-material-other'),
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
      if (heading) heading.textContent = on ? Ferum.t('js-write-a-review') : Ferum.t('js-new-post');
      // The submit button's first child is its text node (spinner span follows).
      if (submitBtn && submitBtn.childNodes[0]) {
        submitBtn.childNodes[0].nodeValue = on ? Ferum.t('js-publish-review') : Ferum.t('js-publish');
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
        empty.textContent = Ferum.t('js-no-products-found');
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
          pend.textContent = Ferum.t('js-pending-approval');
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
    var modalEl = document.getElementById('productAddModal');
    var form = document.getElementById('np-form');
    var brandSel = document.getElementById('np-brand');
    var errEl = document.getElementById('np-error');
    var submitBtn = document.getElementById('np-submit');
    var nameEl = document.getElementById('np-name');
    var imageInput = document.getElementById('np-images');
    var imagePreview = document.getElementById('np-image-preview');
    var matSearch = document.getElementById('np-mat-search');
    var matChips = document.getElementById('np-mat-chips');
    var matResults = document.getElementById('np-mat-results');
    var matWrap = document.getElementById('np-mat-wrap');
    var dupesBox = document.getElementById('np-dupes');
    var dupeList = document.getElementById('np-dupe-list');
    var refsLoaded = false;
    var allMaterials = [];
    var pickedMaterials = []; // material ids, in the order the user picked them
    // Photos wait here until the product row exists — the upload endpoint is
    // keyed by product id, which we only get back from the create call.
    var pendingImages = [];
    var _dupeTimer = null;

    function bsModal() { return bootstrap.Modal.getOrCreateInstance(modalEl); }

    // ── Field-level validation feedback ────────────────────────────────────
    function setFieldError(el, feedbackId, msg) {
      el.classList.add('is-invalid');
      var fb = document.getElementById(feedbackId);
      if (fb) fb.textContent = msg;
    }
    function clearErrors() {
      errEl.textContent = '';
      errEl.classList.add('d-none');
      Array.prototype.forEach.call(form.querySelectorAll('.is-invalid'), function (el) {
        el.classList.remove('is-invalid');
      });
      Array.prototype.forEach.call(form.querySelectorAll('.invalid-feedback'), function (el) {
        el.textContent = '';
      });
    }
    function showError(msg) {
      errEl.textContent = msg;
      errEl.classList.remove('d-none');
      errEl.scrollIntoView({ block: 'nearest' });
    }

    // Prices are typed with whatever grouping the user is used to; strip it on
    // read, re-apply it on blur.
    function parsePrice(id) {
      var digits = (document.getElementById(id).value || '').replace(/\D/g, '');
      return digits === '' ? null : parseInt(digits, 10);
    }
    ['np-price-min', 'np-price-max'].forEach(function (id) {
      var el = document.getElementById(id);
      el.addEventListener('blur', function () {
        var digits = el.value.replace(/\D/g, '');
        el.value = digits === '' ? '' : new Intl.NumberFormat('vi-VN').format(parseInt(digits, 10));
      });
    });

    // Brands and materials are both small reference lists — fetch once, on the
    // first time the dialog is opened.
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
        .then(function (body) { allMaterials = body.data || []; })
        .catch(function () { showError(Ferum.t('js-could-not-load-materials')); });
    }

    // ── Materials: chip + typeahead, same interaction as the admin catalogue ─
    function renderMatChips() {
      matChips.innerHTML = '';
      pickedMaterials.forEach(function (id) {
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
          pickedMaterials = pickedMaterials.filter(function (x) { return x !== id; });
          renderMatChips();
        });
        chip.appendChild(rm);
        matChips.appendChild(chip);
      });
    }

    /// Drop the menu upwards when the field is too close to the bottom of the
    /// viewport, so it never lands on top of the dialog's own buttons.
    function placeMatResults() {
      var below = window.innerHeight - matWrap.getBoundingClientRect().bottom;
      matResults.classList.toggle('fr-typeahead-up', below < 240);
    }

    function showMatResults(term) {
      var matches = allMaterials.filter(function (m) {
        return pickedMaterials.indexOf(m.id) === -1 &&
          (!term || m.name.toLowerCase().indexOf(term.toLowerCase()) !== -1);
      }).slice(0, 12);
      matResults.innerHTML = '';
      placeMatResults();
      if (!matches.length) {
        // Silence here reads as a broken control, so say which kind of empty
        // this is: nothing typed matches, or the catalogue has nothing at all.
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
        btn.appendChild(document.createTextNode(m.name));
        var cat = document.createElement('span');
        cat.className = 'text-muted small ms-1';
        cat.textContent = '(' + (_matCatLabels[m.category] || m.category || '') + ')';
        btn.appendChild(cat);
        btn.addEventListener('click', function () {
          pickedMaterials.push(m.id);
          renderMatChips();
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
      if (!matWrap.contains(e.target) && !matResults.contains(e.target)) matResults.classList.add('d-none');
    });

    // ── Duplicate guard ────────────────────────────────────────────────────
    // The catalogue search that led here matched nothing, but people rarely type
    // the same name twice the same way. Re-query as they type and offer the near
    // matches, so an existing product gets reused instead of re-created.
    function renderDupes(items) {
      dupeList.innerHTML = '';
      if (!items.length) { dupesBox.classList.add('d-none'); return; }
      items.slice(0, 4).forEach(function (p) {
        var btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'list-group-item list-group-item-action d-flex justify-content-between align-items-center gap-2';
        var left = document.createElement('span');
        left.className = 'text-truncate';
        left.textContent = p.name;
        var right = document.createElement('span');
        right.className = 'badge text-bg-primary flex-shrink-0';
        right.textContent = Ferum.t('js-use-this-product');
        btn.appendChild(left);
        btn.appendChild(right);
        btn.addEventListener('click', function () {
          bsModal().hide();
          if (_selectProduct) _selectProduct(p.id, p.name);
        });
        dupeList.appendChild(btn);
      });
      dupesBox.classList.remove('d-none');
    }

    nameEl.addEventListener('input', function () {
      nameEl.classList.remove('is-invalid');
      clearTimeout(_dupeTimer);
      var term = nameEl.value.trim();
      if (term.length < 3) { dupesBox.classList.add('d-none'); return; }
      _dupeTimer = setTimeout(function () {
        fetch('/api/products?per_page=4&q=' + encodeURIComponent(term), { headers: { Accept: 'application/json' } })
          .then(function (r) { return r.ok ? r.json() : { data: [] }; })
          .then(function (body) { renderDupes(body.data || []); })
          .catch(function () { dupesBox.classList.add('d-none'); });
      }, 300);
    });

    function renderImagePreview() {
      imagePreview.innerHTML = '';
      pendingImages.forEach(function (p, i) {
        var wrap = document.createElement('div');
        wrap.className = 'fr-thumb';

        var img = document.createElement('img');
        img.src = p.url;
        img.alt = '';
        wrap.appendChild(img);

        var rm = document.createElement('button');
        rm.type = 'button';
        rm.className = 'fr-thumb-remove';
        rm.textContent = '×';
        rm.title = Ferum.t('js-remove-photo');
        rm.setAttribute('aria-label', Ferum.t('js-remove-photo'));
        rm.addEventListener('click', function () {
          URL.revokeObjectURL(pendingImages[i].url);
          pendingImages.splice(i, 1);
          renderImagePreview();
        });
        wrap.appendChild(rm);

        imagePreview.appendChild(wrap);
      });
    }

    if (imageInput) {
      imageInput.addEventListener('change', function () {
        Array.prototype.slice.call(imageInput.files || []).forEach(function (f) {
          pendingImages.push({ file: f, url: URL.createObjectURL(f) });
        });
        imageInput.value = '';
        renderImagePreview();
      });
    }

    /// Upload the staged photos to a freshly created product. Returns how many
    /// failed — the product itself is already saved either way.
    async function uploadPendingImages(productId) {
      var failed = 0;
      for (var i = 0; i < pendingImages.length; i++) {
        var fd = new FormData();
        fd.append('image', pendingImages[i].file);
        try {
          var res = await fetch('/api/products/' + productId + '/media', {
            method: 'POST',
            body: fd,
          });
          if (!res.ok) failed++;
        } catch (_) {
          failed++;
        }
      }
      pendingImages.forEach(function (p) { URL.revokeObjectURL(p.url); });
      pendingImages = [];
      renderImagePreview();
      return failed;
    }

    toggle.addEventListener('click', function () { openAddProduct(); });

    function openAddProduct(presetName) {
      loadRefs();
      if (presetName) nameEl.value = presetName;
      bsModal().show();
    }

    modalEl.addEventListener('shown.bs.modal', function () { nameEl.focus(); });

    form.addEventListener('submit', async function (ev) {
      ev.preventDefault();
      clearErrors();

      var name = nameEl.value.trim();
      if (!name) setFieldError(nameEl, 'np-name-feedback', Ferum.t('js-enter-product-name'));

      var min = parsePrice('np-price-min'), max = parsePrice('np-price-max');
      if (min != null && max != null && min > max) {
        setFieldError(document.getElementById('np-price-min'), 'np-price-feedback', Ferum.t('js-price-from-exceeds-to'));
        document.getElementById('np-price-max').classList.add('is-invalid');
      }

      var firstBad = form.querySelector('.is-invalid');
      if (firstBad) { firstBad.focus(); firstBad.scrollIntoView({ block: 'center' }); return; }

      var payload = {
        name: name,
        product_type: document.getElementById('np-type').value,
        brand_id: brandSel.value || null,
        price_min: min,
        price_max: max,
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
          showError((body.error && body.error.message) || Ferum.t('js-could-not-add-product'));
          submitBtn.disabled = false;
          return;
        }
        var p = body.data || {};
        if (pendingImages.length) {
          var failed = await uploadPendingImages(p.id);
          // The product itself is saved either way, so report the photo failure
          // and still carry on into the review rather than blocking here.
          if (failed) showError(Ferum.t('js-product-added-photos-failed'));
        }
        bsModal().hide();
        if (_selectProduct) _selectProduct(p.id, p.name);
      } catch (_) {
        showError(Ferum.t('js-network-error'));
      } finally {
        submitBtn.disabled = false;
      }
    });

    // Deep-link from the catalog "Suggest a product" CTA:
    // /new-thread?add_product=1[&name=…] opens the add-product dialog straight
    // away, optionally pre-filling the name from the failed catalog search.
    var params = new URLSearchParams(window.location.search);
    if (params.get('add_product')) openAddProduct(params.get('name'));
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
      fail(Ferum.t('js-give-overall-score'));
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
        fail((body.error && body.error.message) || Ferum.t('js-could-not-publish'));
        return;
      }
      var data = await res.json();
      var slug = (data.data && data.data.thread && data.data.thread.slug) || '';
      window.location.href = '/forum/t/' + slug;
    } catch (_) {
      fail(Ferum.t('js-network-error'));
    }
  });
}());
