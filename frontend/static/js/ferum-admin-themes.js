(function () {
  'use strict';

  var modal = document.getElementById('uninstallModal');
  if (!modal) return;

  // Populate modal fields when a trash button opens it.
  modal.addEventListener('show.bs.modal', function (e) {
    var btn  = e.relatedTarget;
    var slug = btn.getAttribute('data-theme-slug') || '';
    var name = btn.getAttribute('data-theme-name') || slug;

    document.getElementById('uninstall-theme-name').textContent = name;
    document.getElementById('uninstall-slug-display').textContent = slug;
    document.getElementById('uninstall-form').action = '/admin/themes/' + slug + '/delete';

    var input      = document.getElementById('uninstall-confirm-input');
    var confirmBtn = document.getElementById('uninstall-confirm-btn');
    input.value       = '';
    input.placeholder = slug;
    confirmBtn.disabled = true;

    input.oninput = function () {
      confirmBtn.disabled = input.value.trim() !== slug;
    };
  });

  // Reset state when modal closes.
  modal.addEventListener('hidden.bs.modal', function () {
    document.getElementById('uninstall-confirm-input').value = '';
    document.getElementById('uninstall-confirm-btn').disabled = true;
  });

}());
