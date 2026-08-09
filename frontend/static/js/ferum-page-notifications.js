(function () {
  'use strict';

  // This page used to carry its own copy of `relativeTime`, with the strings
  // hardcoded in English and `toLocaleDateString(undefined, …)` following the
  // *browser's* locale rather than the page's. A Vietnamese reader got "3d ago"
  // and "Aug 8, 2026" here while every other page spoke Vietnamese.
  //
  // `Ferum.timeAgo` is the same logic, translated through the `js-time-*` keys
  // and rendered in the viewer's configured timezone.
  document.querySelectorAll('.fr-notif-time[data-ts]').forEach(function (el) {
    var ts = el.dataset.ts;
    el.textContent = Ferum.timeAgo(ts);
    el.title = Ferum.formatAbs(ts, 'datetime');
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
        Ferum.toast(Ferum.t('js-could-not-mark-read'), true);
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
        Ferum.toast(Ferum.t('js-could-not-mark-all-read'), true);
      }
    });
  }
}());
