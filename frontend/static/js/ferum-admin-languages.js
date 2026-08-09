// Admin → Languages: enable/disable a locale, or set the site default.
//
// Deliberately a full reload after each change rather than patching the row in
// place: enabling a language changes which switcher appears in the nav, and
// making one the default re-labels another row. Re-rendering from the server is
// both simpler and guaranteed consistent.
(function () {
  var buttons = document.querySelectorAll('[data-lang-action]');
  if (!buttons.length) return;

  var feedback = document.getElementById('lang-feedback');

  function show(kind, message) {
    if (!feedback) return;
    feedback.innerHTML =
      '<div class="alert alert-' + kind + ' mb-0">' + message + '</div>';
  }

  buttons.forEach(function (btn) {
    btn.addEventListener('click', function () {
      var tag = btn.getAttribute('data-tag');
      var action = btn.getAttribute('data-lang-action');

      var payload = {};
      if (action === 'enable') payload.enabled = true;
      else if (action === 'disable') payload.enabled = false;
      else if (action === 'make-default') payload.make_default = true;

      btn.disabled = true;

      FerumApi.http.patch('/api/admin/languages/' + encodeURIComponent(tag), payload)
        .then(function (res) {
          if (res.ok) {
            window.location.reload();
            return null;
          }
          // The server refuses to disable the last language or the site
          // default; surface its message rather than a generic failure.
          return res.json().then(function (data) {
            throw new Error(
              Ferum.errorMessage(data) || 'Request failed.'
            );
          });
        })
        .catch(function (err) {
          btn.disabled = false;
          show('danger', err.message || 'Could not update the language.');
        });
    });
  });
}());
