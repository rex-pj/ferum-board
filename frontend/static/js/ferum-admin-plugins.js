(function () {
  'use strict';

  var modal = document.getElementById('uninstallPluginModal');
  if (!modal) return;

  // Populate modal fields when a trash button opens it.
  modal.addEventListener('show.bs.modal', function (e) {
    var btn  = e.relatedTarget;
    var slug = btn.getAttribute('data-plugin-slug') || '';
    var name = btn.getAttribute('data-plugin-name') || slug;

    document.getElementById('uninstall-plugin-name').textContent = name;
    document.getElementById('uninstall-plugin-slug-display').textContent = slug;
    document.getElementById('uninstall-plugin-form').action =
      '/admin/plugins/' + slug + '/uninstall';

    var input      = document.getElementById('uninstall-plugin-confirm-input');
    var confirmBtn = document.getElementById('uninstall-plugin-confirm-btn');
    input.value       = '';
    input.placeholder = slug;
    confirmBtn.disabled = true;

    input.oninput = function () {
      confirmBtn.disabled = input.value.trim() !== slug;
    };
  });

  // Reset state when modal closes.
  modal.addEventListener('hidden.bs.modal', function () {
    document.getElementById('uninstall-plugin-confirm-input').value = '';
    document.getElementById('uninstall-plugin-confirm-btn').disabled = true;
  });

}());
