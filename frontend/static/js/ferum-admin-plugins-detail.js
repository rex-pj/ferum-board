(function () {
  'use strict';

  // ── Uninstall slug confirmation ────────────────────────────────────
  var confirmInput = document.getElementById('confirm-slug-input');
  var confirmBtn   = document.getElementById('uninstall-confirm-btn');
  var uninstallForm = document.getElementById('uninstall-form');

  if (confirmInput && confirmBtn && uninstallForm) {
    var expectedSlug = confirmInput.getAttribute('placeholder') || '';
    confirmInput.addEventListener('input', function () {
      confirmBtn.disabled = confirmInput.value.trim() !== expectedSlug;
    });
  }

  // ── Config JSON validation ─────────────────────────────────────────
  var configForm    = document.getElementById('config-form');
  var configEditor  = document.getElementById('plugin-config-editor');
  var jsonError     = document.getElementById('json-error');
  var saveConfigBtn = document.getElementById('save-config-btn');

  if (configForm && configEditor) {
    configEditor.addEventListener('input', function () {
      try {
        JSON.parse(configEditor.value);
        configEditor.classList.remove('is-invalid');
        if (jsonError) jsonError.textContent = '';
        if (saveConfigBtn) saveConfigBtn.disabled = false;
      } catch (e) {
        configEditor.classList.add('is-invalid');
        if (jsonError) jsonError.textContent = 'Invalid JSON: ' + e.message;
        if (saveConfigBtn) saveConfigBtn.disabled = true;
      }
    });

    configForm.addEventListener('submit', function (e) {
      try {
        JSON.parse(configEditor.value);
      } catch (e) {
        e.preventDefault();
        configEditor.classList.add('is-invalid');
        if (jsonError) jsonError.textContent = 'Fix JSON errors before saving.';
      }
    });
  }

}());
