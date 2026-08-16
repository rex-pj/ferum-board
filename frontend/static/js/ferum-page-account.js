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
      showFeedback('profile-feedback', 'danger', Ferum.t('js-invalid-website-url'));
      return;
    }
    setSpinner('profile-submit-btn', 'profile-spinner', true);
    try {
      var res = await FerumApi.users.updateProfile({
        display_name: document.getElementById('display_name').value || null,
        bio:          document.getElementById('bio').value          || null,
        website:      document.getElementById('website').value      || null,
      });
      if (res.ok) showFeedback('profile-feedback', 'success', Ferum.t('js-profile-saved'));
      else {
        var b = await res.json().catch(function () { return {}; });
        showFeedback('profile-feedback', 'danger', Ferum.errorMessage(b) || Ferum.t('js-failed-to-save'));
      }
    } catch (_) { showFeedback('profile-feedback', 'danger', Ferum.t('js-network-error')); }
    setSpinner('profile-submit-btn', 'profile-spinner', false);
  });

  // ── Avatar / Cover removal ────────────────────────────────────────
  //
  // Uploading lives in <ferum-image-cropper>, which owns pick → frame → POST.
  // Only removal is left here: it needs no image handling, and routing it
  // through the widget would give the widget a second, unrelated job.

  async function removeAvatar() {
    try {
      var res = await FerumApi.users.removeAvatar();
      if (res.ok) {
        showFeedback('profile-feedback', 'success', Ferum.t('js-avatar-removed'));
        setTimeout(function () { location.reload(); }, 1200);
      } else showFeedback('profile-feedback', 'danger', Ferum.t('js-failed-remove-avatar'));
    } catch (_) { showFeedback('profile-feedback', 'danger', Ferum.t('js-network-error')); }
  }

  async function removeCover() {
    try {
      var res = await FerumApi.users.removeCover();
      if (res.ok) {
        showFeedback('profile-feedback', 'success', Ferum.t('js-cover-removed'));
        setTimeout(function () { location.reload(); }, 1200);
      } else showFeedback('profile-feedback', 'danger', Ferum.t('js-failed-remove-cover'));
    } catch (_) { showFeedback('profile-feedback', 'danger', Ferum.t('js-network-error')); }
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
      if (mismatch) mismatch.textContent = Ferum.t('js-password-missing-digit-special');
      cpEl.classList.add('is-invalid'); cpEl.classList.remove('is-valid');
      if (submitBtn) submitBtn.disabled = true;
    } else if (cp.length > 0 && np !== cp) {
      if (mismatch) mismatch.textContent = Ferum.t('js-passwords-do-not-match');
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
        showFeedback('password-feedback', 'success', Ferum.t('js-password-changed'));
        document.getElementById('password-form').reset();
        document.getElementById('confirm_password')?.classList.remove('is-valid');
      } else {
        var b = await res.json().catch(function () { return {}; });
        showFeedback('password-feedback', 'danger', Ferum.errorMessage(b) || Ferum.t('js-failed-change-password'));
      }
    } catch (_) { showFeedback('password-feedback', 'danger', Ferum.t('js-network-error')); }
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

  // ── Timezone <select> ──────────────────────────────────────────────
  // Server renders "match my device" plus whatever the user already chose; the
  // shared filler adds the rest of the IANA list from the browser.
  Ferum.fillTimezoneSelect(document.getElementById('timezone-val'));

  document.getElementById('prefs-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    var theme    = document.getElementById('theme-val').value;
    var fontSize = document.getElementById('font-size-val').value;
    var layout   = document.getElementById('layout-val').value;
    // The language <select> only exists when the site has more than one locale.
    var localeEl = document.getElementById('locale-val');
    // Empty string means "no explicit choice"; send null so the server clears the
    // stored preference rather than pinning the user to the default language.
    var locale   = localeEl ? (localeEl.value || null) : undefined;
    // Normalize both sides to a string: `locale` is null for "no choice" while
    // the attribute is "", and a raw !== would report a change on every save.
    var localeChanged =
      localeEl && (locale || '') !== (localeEl.getAttribute('data-initial') || '');

    // Same empty-means-null contract as `locale`: clearing it returns the user
    // to their device's zone rather than pinning them to whatever it is today.
    var tzEl = document.getElementById('timezone-val');
    var timezone = tzEl ? (tzEl.value || null) : undefined;
    // Every server-rendered timestamp on the page was formatted with the old
    // zone, so a change needs the same reload the language change does.
    var tzChanged =
      tzEl && (timezone || '') !== (tzEl.getAttribute('data-current') || '');

    // Sent as one object rather than two flags: the server stores the whole
    // `email_notifications` JSON, so a partial payload would drop the key it left
    // out. Both switches are always read, whatever their state.
    var replyEl   = document.getElementById('email-notify-reply');
    var mentionEl = document.getElementById('email-notify-mention');

    setSpinner('prefs-submit-btn', 'prefs-spinner', true);
    try {
      var payload = { theme: theme, font_size: fontSize, layout: layout };
      if (locale !== undefined) payload.locale = locale;
      if (timezone !== undefined) payload.timezone = timezone;
      if (replyEl && mentionEl) {
        payload.email_notifications = {
          reply: replyEl.checked,
          mention: mentionEl.checked,
        };
      }
      var res = await FerumApi.users.updatePreferences(payload);
      if (res.ok) {
        showFeedback('prefs-feedback', 'success', Ferum.t('js-preferences-saved'));
        // Theme/font/layout are applied live below, but language and timezone
        // are baked into the server-rendered HTML (the <meta name="ferum-tz">
        // that every date helper reads) — the only way to show either is a
        // reload.
        if (localeChanged || tzChanged) {
          window.location.reload();
          return;
        }
        try {
          localStorage.setItem('ferum-theme', theme);
          localStorage.setItem('ferum-font-size', fontSize);
          localStorage.setItem('ferum-layout', layout);
        } catch (_) {}
        // Through FerumTheme so 'auto' resolves against the OS exactly as it
        // does on page load. Removing the attribute instead (what this did
        // before) matches neither token block in the CSS and always renders
        // light, so picking "auto" on a dark-mode machine appeared to do
        // nothing until the next reload.
        if (window.FerumTheme) window.FerumTheme.apply(theme);
        else document.documentElement.setAttribute('data-bs-theme', theme);
        if (fontSize && fontSize !== 'medium') document.documentElement.setAttribute('data-font-size', fontSize);
        else document.documentElement.removeAttribute('data-font-size');
        if (layout && layout !== 'comfortable') document.documentElement.setAttribute('data-layout', layout);
        else document.documentElement.removeAttribute('data-layout');
      } else {
        showFeedback('prefs-feedback', 'danger', Ferum.t('js-failed-save-preferences'));
      }
    } catch (_) { showFeedback('prefs-feedback', 'danger', Ferum.t('js-network-error')); }
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
          if (!pr.ok) { Ferum.toast(Ferum.t('js-failed-unwatch'), true); return; }
          var prefs = (await pr.json()).data || {};
          var res   = await FerumApi.users.updatePreferences(Object.assign({}, prefs, {
            watched_categories: (prefs.watched_categories || []).filter(function (x) { return x !== id; }),
          }));
          if (res.ok) this.watched = this.watched.filter(function (c) { return c.id !== id; });
          else Ferum.toast(Ferum.t('js-failed-unwatch'), true);
        } catch (_) { Ferum.toast(Ferum.t('js-network-error'), true); }
        this.removing = this.removing.filter(function (x) { return x !== id; });
      },
      async unmute(id) {
        if (this.removing.includes(id)) return;
        this.removing = this.removing.concat([id]);
        try {
          var pr = await FerumApi.users.getPreferences();
          if (!pr.ok) { Ferum.toast(Ferum.t('js-failed-unmute'), true); return; }
          var prefs = (await pr.json()).data || {};
          var res   = await FerumApi.users.updatePreferences(Object.assign({}, prefs, {
            muted_categories: (prefs.muted_categories || []).filter(function (x) { return x !== id; }),
          }));
          if (res.ok) this.muted = this.muted.filter(function (c) { return c.id !== id; });
          else Ferum.toast(Ferum.t('js-failed-unmute'), true);
        } catch (_) { Ferum.toast(Ferum.t('js-network-error'), true); }
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

}());
