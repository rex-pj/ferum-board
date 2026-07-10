(function () {
  'use strict';

  // ── Account tab state (Alpine factory) ───────────────────────────────
  // Supports deep-linking via URL hash: /account#security, /account#preferences
  window.accountTabState = function (dnLen, bioLen) {
    var VALID_TABS = ['profile', 'security', 'preferences', 'reports'];
    var hashTab = window.location.hash.slice(1);
    return {
      active: VALID_TABS.includes(hashTab) ? hashTab : 'profile',
      dnLen: dnLen,
      bioLen: bioLen,
      init: function () {},
      setTab: function (tab) {
        this.active = tab;
        history.replaceState(null, '', '#' + tab);
      },
    };
  };

  function showFeedback(id, type, msg) { Ferum.showFeedback(id, type, msg); }

  function setSpinner(btnId, spinnerId, loading) {
    var btn     = document.getElementById(btnId);
    var spinner = document.getElementById(spinnerId);
    if (btn)     btn.disabled = loading;
    if (spinner) spinner.classList.toggle('d-none', !loading);
  }

  // ── Website URL validation ────────────────────────────────────────
  var websiteInput = document.getElementById('website');
  function validateWebsite() {
    if (!websiteInput) return true;
    var val = websiteInput.value.trim();
    if (val === '') { websiteInput.classList.remove('is-invalid', 'is-valid'); return true; }
    var ok = /^https?:\/\//.test(val);
    websiteInput.classList.toggle('is-invalid', !ok);
    websiteInput.classList.toggle('is-valid', ok);
    return ok;
  }
  if (websiteInput) {
    websiteInput.addEventListener('input', validateWebsite);
    websiteInput.addEventListener('blur', validateWebsite);
  }

  // ── Profile form ──────────────────────────────────────────────────
  document.getElementById('profile-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    if (!validateWebsite()) {
      showFeedback('profile-feedback', 'danger', 'Please enter a valid URL starting with http:// or https://');
      return;
    }
    setSpinner('profile-submit-btn', 'profile-spinner', true);
    try {
      var res = await FerumApi.users.updateProfile({
        display_name: document.getElementById('display_name').value || null,
        bio:          document.getElementById('bio').value          || null,
        website:      document.getElementById('website').value      || null,
      });
      if (res.ok) showFeedback('profile-feedback', 'success', 'Profile saved.');
      else {
        var b = await res.json().catch(function () { return {}; });
        showFeedback('profile-feedback', 'danger', (b.error && b.error.message) || 'Failed to save.');
      }
    } catch (_) { showFeedback('profile-feedback', 'danger', 'Network error.'); }
    setSpinner('profile-submit-btn', 'profile-spinner', false);
  });

  // ── Avatar / Cover uploads ────────────────────────────────────────

  function setUploadStatus(statusId, html) {
    var el = document.getElementById(statusId);
    if (el) el.innerHTML = html;
  }

  var ACCEPTED_IMAGE_TYPES = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];

  async function uploadAvatar(input) {
    var file = input.files && input.files[0];
    if (!file) return;
    if (!ACCEPTED_IMAGE_TYPES.includes(file.type)) {
      showFeedback('profile-feedback', 'danger', 'Avatar must be a JPEG, PNG, GIF, or WebP image.');
      input.value = '';
      return;
    }
    if (file.size > 5 * 1024 * 1024) {
      showFeedback('profile-feedback', 'danger', 'Avatar must be under 5 MB.');
      input.value = '';
      return;
    }
    setUploadStatus('avatar-upload-status', '<i class="fa-solid fa-spinner fa-spin me-1"></i>Uploading…');
    var fd = new FormData();
    fd.append('file', file);
    try {
      var res = await FerumApi.users.uploadAvatar(fd);
      if (res.ok) {
        showFeedback('profile-feedback', 'success', 'Avatar updated.');
        setTimeout(function () { location.reload(); }, 1200);
      } else {
        var b = await res.json().catch(function () { return {}; });
        showFeedback('profile-feedback', 'danger', (b.error && b.error.message) || 'Upload failed.');
        setUploadStatus('avatar-upload-status', '');
      }
    } catch (_) {
      showFeedback('profile-feedback', 'danger', 'Network error.');
      setUploadStatus('avatar-upload-status', '');
    }
    input.value = '';
  }

  async function removeAvatar() {
    try {
      var res = await FerumApi.users.removeAvatar();
      if (res.ok) {
        showFeedback('profile-feedback', 'success', 'Avatar removed.');
        setTimeout(function () { location.reload(); }, 1200);
      } else showFeedback('profile-feedback', 'danger', 'Failed to remove avatar.');
    } catch (_) { showFeedback('profile-feedback', 'danger', 'Network error.'); }
  }

  async function uploadCover(input) {
    var file = input.files && input.files[0];
    if (!file) return;
    if (!ACCEPTED_IMAGE_TYPES.includes(file.type)) {
      showFeedback('profile-feedback', 'danger', 'Cover must be a JPEG, PNG, GIF, or WebP image.');
      input.value = '';
      return;
    }
    if (file.size > 8 * 1024 * 1024) {
      showFeedback('profile-feedback', 'danger', 'Cover image must be under 8 MB.');
      input.value = '';
      return;
    }
    setUploadStatus('cover-upload-status', '<i class="fa-solid fa-spinner fa-spin me-1"></i>Uploading…');
    var fd = new FormData();
    fd.append('file', file);
    try {
      var res = await FerumApi.users.uploadCover(fd);
      if (res.ok) {
        showFeedback('profile-feedback', 'success', 'Cover updated.');
        setTimeout(function () { location.reload(); }, 1200);
      } else {
        var b = await res.json().catch(function () { return {}; });
        showFeedback('profile-feedback', 'danger', (b.error && b.error.message) || 'Upload failed.');
        setUploadStatus('cover-upload-status', '');
      }
    } catch (_) {
      showFeedback('profile-feedback', 'danger', 'Network error.');
      setUploadStatus('cover-upload-status', '');
    }
    input.value = '';
  }

  async function removeCover() {
    try {
      var res = await FerumApi.users.removeCover();
      if (res.ok) {
        showFeedback('profile-feedback', 'success', 'Cover removed.');
        setTimeout(function () { location.reload(); }, 1200);
      } else showFeedback('profile-feedback', 'danger', 'Failed to remove cover.');
    } catch (_) { showFeedback('profile-feedback', 'danger', 'Network error.'); }
  }

  // ── Password form ─────────────────────────────────────────────────
  function passwordIsStrong(pwd) {
    return pwd.length >= 8 && /[0-9]/.test(pwd) && /[^a-zA-Z0-9]/.test(pwd);
  }

  function checkPasswords() {
    var np       = document.getElementById('new_password').value;
    var cp       = document.getElementById('confirm_password').value;
    var cpEl     = document.getElementById('confirm_password');
    var mismatch = document.getElementById('pw-mismatch');
    var submitBtn = document.getElementById('pw-submit-btn');

    if (np.length > 0 && !passwordIsStrong(np)) {
      if (mismatch) mismatch.textContent = 'Must include a digit and a special character.';
      cpEl.classList.add('is-invalid'); cpEl.classList.remove('is-valid');
      if (submitBtn) submitBtn.disabled = true;
    } else if (cp.length > 0 && np !== cp) {
      if (mismatch) mismatch.textContent = 'Passwords do not match.';
      cpEl.classList.add('is-invalid'); cpEl.classList.remove('is-valid');
      if (submitBtn) submitBtn.disabled = true;
    } else if (cp.length >= 8 && np === cp && passwordIsStrong(np)) {
      cpEl.classList.remove('is-invalid'); cpEl.classList.add('is-valid');
      if (submitBtn) submitBtn.disabled = false;
    } else {
      cpEl.classList.remove('is-invalid', 'is-valid');
      if (submitBtn) submitBtn.disabled = false;
    }
  }

  ['new_password', 'confirm_password'].forEach(function (id) {
    document.getElementById(id)?.addEventListener('input', checkPasswords);
  });
  Ferum.initPasswordStrength('new_password', 'new-pw-strength-fill', 'new-pw-strength-text');

  document.getElementById('password-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    setSpinner('pw-submit-btn', 'pw-spinner', true);
    try {
      var res = await FerumApi.users.changePassword(
        document.getElementById('current_password').value,
        document.getElementById('new_password').value,
      );
      if (res.ok) {
        showFeedback('password-feedback', 'success', 'Password changed successfully.');
        document.getElementById('password-form').reset();
        document.getElementById('confirm_password')?.classList.remove('is-valid');
      } else {
        var b = await res.json().catch(function () { return {}; });
        showFeedback('password-feedback', 'danger', (b.error && b.error.message) || 'Failed to change password.');
      }
    } catch (_) { showFeedback('password-feedback', 'danger', 'Network error.'); }
    setSpinner('pw-submit-btn', 'pw-spinner', false);
  });

  // ── Preferences ───────────────────────────────────────────────────
  function setTheme(t, btn) {
    var val = document.getElementById('theme-val');
    if (val) val.value = t;
    document.querySelectorAll('#theme-btns .btn').forEach(function (b) {
      b.classList.remove('btn-primary'); b.classList.add('btn-outline-secondary');
    });
    if (btn) { btn.classList.remove('btn-outline-secondary'); btn.classList.add('btn-primary'); }
  }

  function setFontSize(s, btn) {
    var val = document.getElementById('font-size-val');
    if (val) val.value = s;
    document.querySelectorAll('#font-btns .btn').forEach(function (b) {
      b.classList.remove('btn-primary'); b.classList.add('btn-outline-secondary');
    });
    if (btn) { btn.classList.remove('btn-outline-secondary'); btn.classList.add('btn-primary'); }
  }

  function setLayout(l, btn) {
    var val = document.getElementById('layout-val');
    if (val) val.value = l;
    document.querySelectorAll('#layout-btns .btn').forEach(function (b) {
      b.classList.remove('btn-primary'); b.classList.add('btn-outline-secondary');
    });
    if (btn) { btn.classList.remove('btn-outline-secondary'); btn.classList.add('btn-primary'); }
  }

  document.getElementById('prefs-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    var theme    = document.getElementById('theme-val').value;
    var fontSize = document.getElementById('font-size-val').value;
    var layout   = document.getElementById('layout-val').value;
    setSpinner('prefs-submit-btn', 'prefs-spinner', true);
    try {
      var res = await FerumApi.users.updatePreferences({ theme: theme, font_size: fontSize, layout: layout });
      if (res.ok) {
        showFeedback('prefs-feedback', 'success', 'Preferences saved.');
        try {
          localStorage.setItem('ferum-theme', theme);
          localStorage.setItem('ferum-font-size', fontSize);
          localStorage.setItem('ferum-layout', layout);
        } catch (_) {}
        if (theme === 'auto') document.documentElement.removeAttribute('data-bs-theme');
        else document.documentElement.setAttribute('data-bs-theme', theme);
        if (fontSize && fontSize !== 'medium') document.documentElement.setAttribute('data-font-size', fontSize);
        else document.documentElement.removeAttribute('data-font-size');
        if (layout && layout !== 'comfortable') document.documentElement.setAttribute('data-layout', layout);
        else document.documentElement.removeAttribute('data-layout');
      } else {
        showFeedback('prefs-feedback', 'danger', 'Failed to save preferences.');
      }
    } catch (_) { showFeedback('prefs-feedback', 'danger', 'Network error.'); }
    setSpinner('prefs-submit-btn', 'prefs-spinner', false);
  });

  // ── Watched / Muted categories (Alpine factory) ───────────────────
  // ── My Reports tab (Alpine factory) ──────────────────────────────────
  window.myReportsState = function () {
    return {
      reports: [],
      loading: true,
      loaded: false,
      async load() {
        if (this.loaded) return;
        this.loaded = true;
        try {
          var res = await FerumApi.reports.mine();
          if (res.ok) this.reports = (await res.json()).data || [];
        } catch (_) {}
        this.loading = false;
      },
    };
  };

  window.watchedMuted = function () {
    return {
      watched: [],
      muted: [],
      removing: [],
      async init() {
        try {
          var results = await Promise.all([FerumApi.users.getPreferences(), FerumApi.categories.list()]);
          var pr = results[0]; var cr = results[1];
          if (!pr.ok || !cr.ok) return;
          var prefs = (await pr.json()).data || {};
          var cats  = (await cr.json()).data || [];
          this.watched = cats.filter(function (c) { return (prefs.watched_categories || []).includes(c.id); });
          this.muted   = cats.filter(function (c) { return (prefs.muted_categories   || []).includes(c.id); });
        } catch (_) {}
      },
      async unwatch(id) {
        if (this.removing.includes(id)) return;
        this.removing = this.removing.concat([id]);
        try {
          var pr = await FerumApi.users.getPreferences();
          if (!pr.ok) { Ferum.toast('Failed to unwatch category. Please try again.', true); return; }
          var prefs = (await pr.json()).data || {};
          var res   = await FerumApi.users.updatePreferences(Object.assign({}, prefs, {
            watched_categories: (prefs.watched_categories || []).filter(function (x) { return x !== id; }),
          }));
          if (res.ok) this.watched = this.watched.filter(function (c) { return c.id !== id; });
          else Ferum.toast('Failed to unwatch category. Please try again.', true);
        } catch (_) { Ferum.toast('Network error. Please try again.', true); }
        this.removing = this.removing.filter(function (x) { return x !== id; });
      },
      async unmute(id) {
        if (this.removing.includes(id)) return;
        this.removing = this.removing.concat([id]);
        try {
          var pr = await FerumApi.users.getPreferences();
          if (!pr.ok) { Ferum.toast('Failed to unmute category. Please try again.', true); return; }
          var prefs = (await pr.json()).data || {};
          var res   = await FerumApi.users.updatePreferences(Object.assign({}, prefs, {
            muted_categories: (prefs.muted_categories || []).filter(function (x) { return x !== id; }),
          }));
          if (res.ok) this.muted = this.muted.filter(function (c) { return c.id !== id; });
          else Ferum.toast('Failed to unmute category. Please try again.', true);
        } catch (_) { Ferum.toast('Network error. Please try again.', true); }
        this.removing = this.removing.filter(function (x) { return x !== id; });
      },
    };
  };

  // ── Event delegation for inline-action buttons ────────────────────
  document.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-account-action]');
    if (!btn) return;
    switch (btn.dataset.accountAction) {
      case 'remove-cover':   removeCover(); break;
      case 'remove-avatar':  removeAvatar(); break;
      case 'set-theme':      setTheme(btn.dataset.value, btn); break;
      case 'set-font-size':  setFontSize(btn.dataset.value, btn); break;
      case 'set-layout':     setLayout(btn.dataset.value, btn); break;
    }
  });

  document.getElementById('cover-upload')?.addEventListener('change', function () { uploadCover(this); });
  document.getElementById('avatar-upload')?.addEventListener('change', function () { uploadAvatar(this); });
}());
