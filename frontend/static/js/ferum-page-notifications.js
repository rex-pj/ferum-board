(function () {
  'use strict';

  function relativeTime(iso) {
    var diff = Math.floor((Date.now() - new Date(iso)) / 1000);
    if (diff < 60)     return 'just now';
    if (diff < 3600)   return Math.floor(diff / 60) + 'm ago';
    if (diff < 86400)  return Math.floor(diff / 3600) + 'h ago';
    if (diff < 604800) return Math.floor(diff / 86400) + 'd ago';
    return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
  }

  document.querySelectorAll('.fr-notif-time[data-ts]').forEach(function (el) {
    el.textContent = relativeTime(el.dataset.ts);
  });

  document.querySelectorAll('.mark-read-btn').forEach(function (btn) {
    btn.addEventListener('click', async function (e) {
      e.preventDefault();
      e.stopPropagation();
      btn.disabled = true;
      var id  = btn.dataset.id;
      var res = await FerumApi.notifications.markRead(id);
      if (res.ok) {
        var item = document.getElementById('notif-' + id);
        if (item) item.classList.remove('fr-notif-row--unread');
        btn.remove();
      } else {
        btn.disabled = false;
        Ferum.toast('Could not mark as read. Please try again.', true);
      }
    });
  });

  var markAllBtn = document.getElementById('mark-all-btn');
  if (markAllBtn) {
    markAllBtn.addEventListener('click', async function () {
      markAllBtn.disabled = true;
      var res = await FerumApi.notifications.markAllRead();
      if (res.ok) window.location.reload();
      else {
        markAllBtn.disabled = false;
        Ferum.toast('Could not mark all as read. Please try again.', true);
      }
    });
  }
}());
