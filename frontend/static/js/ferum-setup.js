(function () {
  'use strict';

  var currentStep = 1;

  // ── Step navigation ────────────────────────────────────────────────
  function goToStep(n) {
    for (var i = 1; i <= 3; i++) {
      var panel = document.getElementById('panel-' + i);
      var sp    = document.getElementById('sp-' + i);
      panel.classList.toggle('active', i === n);
      sp.classList.remove('active', 'completed');
      if (i < n)  sp.classList.add('completed');
      if (i === n) sp.classList.add('active');
    }
    currentStep = n;
    window.scrollTo(0, 0);
  }

  // Back buttons
  document.querySelectorAll('[data-back]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      goToStep(parseInt(btn.dataset.back, 10));
    });
  });

  // ── Step 1 validation ──────────────────────────────────────────────
  document.getElementById('btn-next-1').addEventListener('click', function () {
    var username = document.getElementById('username').value.trim();
    var email    = document.getElementById('email').value.trim();
    var password = document.getElementById('password').value;
    var ok = true;

    function setError(id, msg) {
      var el = document.getElementById(id);
      el.classList.toggle('is-invalid', !!msg);
      var existing = el.closest('.mb-3') ? el.closest('.mb-3').querySelector('.invalid-feedback') : null;
      if (msg) {
        if (!existing) {
          existing = document.createElement('div');
          existing.className = 'invalid-feedback';
          (el.closest('.input-group') || el).after(existing);
        }
        existing.textContent = msg;
      } else if (existing) {
        existing.textContent = '';
      }
      if (msg) ok = false;
    }

    setError('username', !username ? 'Username is required.' :
      !/^[a-zA-Z0-9_\-]{3,30}$/.test(username) ? 'Only letters, numbers, _ and - (3–30 chars).' : '');
    setError('email', !email ? 'Email is required.' :
      !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) ? 'Enter a valid email address.' : '');
    setError('password', !password ? 'Password is required.' :
      password.length < 8 ? 'Password must be at least 8 characters.' : '');

    if (ok) goToStep(2);
  });

  // ── Step 2 validation ──────────────────────────────────────────────
  document.getElementById('btn-next-2').addEventListener('click', function () {
    var siteName = document.getElementById('site_name').value.trim();
    var el = document.getElementById('site_name');
    el.classList.remove('is-invalid');
    var existing = el.closest('.mb-3').querySelector('.invalid-feedback');
    if (!siteName) {
      el.classList.add('is-invalid');
      if (!existing) {
        existing = document.createElement('div');
        existing.className = 'invalid-feedback';
        el.after(existing);
      }
      existing.textContent = 'Forum name is required.';
      return;
    }
    if (existing) existing.textContent = '';

    document.getElementById('r-username').textContent  = document.getElementById('username').value;
    document.getElementById('r-email').textContent     = document.getElementById('email').value;
    document.getElementById('r-site-name').textContent = siteName;
    var tagline = document.getElementById('site_tagline').value.trim();
    document.getElementById('r-tagline').textContent   = tagline || 'None';
    document.getElementById('r-seed').textContent      = document.getElementById('seed_example_data').checked ? 'Yes' : 'No';

    goToStep(3);
  });

  // ── Submit ─────────────────────────────────────────────────────────
  document.getElementById('submit-btn').addEventListener('click', async function () {
    var btn     = document.getElementById('submit-btn');
    var spinner = document.getElementById('btn-spinner');
    var errEl   = document.getElementById('setup-error');

    btn.disabled = true;
    spinner.classList.remove('d-none');
    errEl.classList.add('d-none');

    var tagline = document.getElementById('site_tagline').value.trim();
    var data = {
      admin_username:    document.getElementById('username').value,
      admin_email:       document.getElementById('email').value,
      admin_password:    document.getElementById('password').value,
      seed_example_data: document.getElementById('seed_example_data').checked,
      config: {
        site_name:    document.getElementById('site_name').value || null,
        site_tagline: tagline || null,
      },
    };

    try {
      var res = await FerumApi.setup.run(data);
      if (res.ok) {
        document.getElementById('btn-text').innerHTML =
          '<i class="fa-solid fa-check me-2"></i>Done! Redirecting…';
        btn.classList.replace('btn-primary', 'btn-success');
        setTimeout(function () { window.location.href = '/admin/dashboard'; }, 800);
      } else {
        var body = await res.json().catch(function () { return {}; });
        errEl.textContent = (body && body.error && body.error.message) || 'Setup failed. Please try again.';
        errEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
      }
    } catch (_) {
      errEl.textContent = 'Network error. Is the server running?';
      errEl.classList.remove('d-none');
      btn.disabled = false;
      spinner.classList.add('d-none');
    }
  });

  // ── Password show/hide ─────────────────────────────────────────────
  document.getElementById('toggle-password').addEventListener('click', function () {
    var input = document.getElementById('password');
    var icon  = document.getElementById('eye-icon');
    var show  = input.type === 'password';
    input.type = show ? 'text' : 'password';
    icon.className = show ? 'fa-solid fa-eye-slash fa-sm' : 'fa-solid fa-eye fa-sm';
    this.setAttribute('aria-label', show ? 'Hide password' : 'Show password');
  });

  // ── Password strength ──────────────────────────────────────────────
  document.getElementById('password').addEventListener('input', function () {
    var val  = this.value;
    var fill = document.getElementById('strength-fill');
    var text = document.getElementById('strength-text');

    if (!val) {
      fill.style.width = '0';
      fill.style.background = '';
      text.textContent = '';
      text.style.color = '';
      return;
    }

    var score = 0;
    if (val.length >= 8)  score++;
    if (val.length >= 12) score++;
    if (/[A-Z]/.test(val) && /[a-z]/.test(val)) score++;
    if (/[0-9]/.test(val)) score++;
    if (/[^A-Za-z0-9]/.test(val)) score++;

    var levels = [
      { pct: '20%', color: '#ef4444', label: 'Very weak'  },
      { pct: '40%', color: '#f97316', label: 'Weak'       },
      { pct: '60%', color: '#eab308', label: 'Fair'       },
      { pct: '80%', color: '#22c55e', label: 'Strong'     },
      { pct: '100%',color: '#10b981', label: 'Very strong'},
    ];
    var lvl = levels[Math.min(score, levels.length) - 1] || levels[0];

    fill.style.width      = lvl.pct;
    fill.style.background = lvl.color;
    text.textContent      = lvl.label;
    text.style.color      = lvl.color;
  });

  // ── Seed data toggle ────────────────────────────────────────────────
  document.getElementById('seed_example_data').addEventListener('change', function () {
    document.getElementById('seed-label').classList.toggle('checked', this.checked);
  });

}());
