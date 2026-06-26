(function () {
  'use strict';

  // ── Login form ─────────────────────────────────────────────────────
  var loginForm = document.getElementById('login-form');
  if (loginForm) {
    Ferum.initPasswordToggle('password');
    loginForm.addEventListener('submit', async function (e) {
      e.preventDefault();
      var btn = document.getElementById('submit-btn');
      var spinner = document.getElementById('btn-spinner');
      var errorEl = document.getElementById('login-error');

      btn.disabled = true;
      spinner.classList.remove('d-none');
      errorEl.classList.add('d-none');

      try {
        var res = await FerumApi.auth.login(
          document.getElementById('email').value,
          document.getElementById('password').value,
        );
        if (res.ok) {
          var raw = new URLSearchParams(location.search).get('next') || '/';
          var next = '/';
          try {
            var parsed = new URL(raw, window.location.origin);
            if (parsed.origin === window.location.origin) {
              next = parsed.pathname + parsed.search + parsed.hash;
            }
          } catch (_) {}
          window.location.href = next;
        } else {
          var body = await res.json().catch(function () { return {}; });
          errorEl.textContent = (body.error && body.error.message) || 'Invalid email or password.';
          errorEl.classList.remove('d-none');
          btn.disabled = false;
          spinner.classList.add('d-none');
        }
      } catch (_) {
        errorEl.textContent = 'Network error. Please try again.';
        errorEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
      }
    });
  }

  // ── Register form ──────────────────────────────────────────────────
  var registerForm = document.getElementById('register-form');
  if (registerForm) {
    Ferum.initPasswordToggle('password');

    function passwordIsStrong(pwd) {
      return pwd.length >= 8 && /[0-9]/.test(pwd) && /[^a-zA-Z0-9]/.test(pwd);
    }

    Ferum.initPasswordStrength('password', 'reg-strength-fill', 'reg-strength-text');

    registerForm.addEventListener('submit', async function (e) {
      e.preventDefault();
      var btn = document.getElementById('submit-btn');
      var spinner = document.getElementById('btn-spinner');
      var errorEl = document.getElementById('register-error');
      var successEl = document.getElementById('register-success');
      var password = document.getElementById('password').value;

      if (!passwordIsStrong(password)) {
        errorEl.textContent = 'Password must be at least 8 characters and include a digit and a special character.';
        errorEl.classList.remove('d-none');
        return;
      }

      btn.disabled = true;
      spinner.classList.remove('d-none');
      errorEl.classList.add('d-none');
      if (successEl) successEl.classList.add('d-none');

      try {
        var res = await FerumApi.auth.register({
          username: document.getElementById('username').value,
          email: document.getElementById('email').value,
          password: password,
        });
        if (res.ok) {
          if (successEl) {
            successEl.textContent = 'Account created! Redirecting to sign in…';
            successEl.classList.remove('d-none');
          }
          setTimeout(function () { window.location.href = '/login'; }, 1500);
        } else {
          var body = await res.json().catch(function () { return {}; });
          errorEl.textContent = (body.error && body.error.message) || 'Registration failed. Please try again.';
          errorEl.classList.remove('d-none');
          btn.disabled = false;
          spinner.classList.add('d-none');
        }
      } catch (_) {
        errorEl.textContent = 'Network error. Please try again.';
        errorEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
      }
    });
  }

  // ── Forgot password form ───────────────────────────────────────────
  var forgotForm = document.getElementById('forgot-form');
  if (forgotForm) {
    forgotForm.addEventListener('submit', async function (e) {
      e.preventDefault();
      var btn = document.getElementById('submit-btn');
      var spinner = document.getElementById('btn-spinner');
      var errorEl = document.getElementById('forgot-error');
      var successEl = document.getElementById('forgot-success');

      btn.disabled = true;
      spinner.classList.remove('d-none');
      errorEl.classList.add('d-none');
      if (successEl) successEl.classList.add('d-none');

      try {
        await FerumApi.auth.forgotPassword(document.getElementById('email').value);
        if (successEl) {
          successEl.textContent = 'If an account with that email exists, a reset link has been sent.';
          successEl.classList.remove('d-none');
        }
        spinner.classList.add('d-none');
        var secs = 30;
        btn.textContent = 'Resend in ' + secs + 's';
        var iv = setInterval(function () {
          secs--;
          if (secs <= 0) {
            clearInterval(iv);
            btn.disabled = false;
            btn.textContent = 'Resend email';
          } else {
            btn.textContent = 'Resend in ' + secs + 's';
          }
        }, 1000);
      } catch (_) {
        errorEl.textContent = 'Network error. Please try again.';
        errorEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
      }
    });
  }

  // ── Reset password form ────────────────────────────────────────────
  var resetForm = document.getElementById('reset-form');
  if (resetForm) {
    Ferum.initPasswordToggle('password');
    Ferum.initPasswordStrength('password', 'reset-strength-fill', 'reset-strength-text');

    function passwordIsStrongReset(pwd) {
      return pwd.length >= 8 && /[0-9]/.test(pwd) && /[^a-zA-Z0-9]/.test(pwd);
    }

    var confirmInput = document.getElementById('password_confirm');
    if (confirmInput) {
      confirmInput.addEventListener('input', function () {
        var pw = document.getElementById('password').value;
        if (!this.value) {
          this.classList.remove('is-invalid', 'is-valid');
        } else if (this.value === pw) {
          this.classList.remove('is-invalid'); this.classList.add('is-valid');
        } else {
          this.classList.remove('is-valid'); this.classList.add('is-invalid');
        }
      });
    }

    resetForm.addEventListener('submit', async function (e) {
      e.preventDefault();
      var btn = document.getElementById('submit-btn');
      var spinner = document.getElementById('btn-spinner');
      var errorEl = document.getElementById('reset-error');
      var successEl = document.getElementById('reset-success');
      var password = document.getElementById('password').value;
      var confirm = document.getElementById('password_confirm').value;

      if (password !== confirm) {
        errorEl.textContent = 'Passwords do not match.';
        errorEl.classList.remove('d-none');
        return;
      }
      if (!passwordIsStrongReset(password)) {
        errorEl.textContent = 'Password must be at least 8 characters and include a digit and a special character.';
        errorEl.classList.remove('d-none');
        return;
      }

      btn.disabled = true;
      spinner.classList.remove('d-none');
      errorEl.classList.add('d-none');

      try {
        var token = document.getElementById('token').value;
        var res = await FerumApi.auth.resetPassword(token, password);
        if (res.ok) {
          if (successEl) {
            successEl.textContent = 'Password updated! Redirecting to sign in…';
            successEl.classList.remove('d-none');
          }
          setTimeout(function () { window.location.href = '/login'; }, 1500);
        } else {
          var body = await res.json().catch(function () { return {}; });
          errorEl.textContent = (body.error && body.error.message) || 'Reset failed. The link may have expired.';
          errorEl.classList.remove('d-none');
          btn.disabled = false;
          spinner.classList.add('d-none');
        }
      } catch (_) {
        errorEl.textContent = 'Network error. Please try again.';
        errorEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
      }
    });
  }
}());
