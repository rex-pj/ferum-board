(function () {
  'use strict';

  // ── Queue (post approval) ─────────────────────────────────────────
  async function approvePost(postId, triggerBtn) {
    if (triggerBtn) triggerBtn.disabled = true;
    try {
      var res = await FerumApi.mod.approvePost(postId);
      if (res.ok) {
        Ferum.showToast('queue-toast', 'Post approved.');
        var el = document.getElementById('queue-item-' + postId);
        if (el) el.remove();
      } else {
        var b = await res.json().catch(function () { return {}; });
        Ferum.showToast('queue-toast', (b.error && b.error.message) || 'Failed to approve.', true);
        if (triggerBtn) triggerBtn.disabled = false;
      }
    } catch (_) {
      Ferum.showToast('queue-toast', Ferum.t('js-network-error'), true);
      if (triggerBtn) triggerBtn.disabled = false;
    }
  }

  async function rejectPost(postId, triggerBtn) {
    var ok = await Ferum.showConfirm('Reject Post', 'Reject and permanently delete this post from the queue?', 'Reject');
    if (!ok) return;
    if (triggerBtn) triggerBtn.disabled = true;
    try {
      var res = await FerumApi.mod.rejectPost(postId);
      if (res.ok) {
        Ferum.showToast('queue-toast', 'Post rejected.');
        var el = document.getElementById('queue-item-' + postId);
        if (el) el.remove();
      } else {
        var b = await res.json().catch(function () { return {}; });
        Ferum.showToast('queue-toast', (b.error && b.error.message) || 'Failed to reject.', true);
        if (triggerBtn) triggerBtn.disabled = false;
      }
    } catch (_) {
      Ferum.showToast('queue-toast', Ferum.t('js-network-error'), true);
      if (triggerBtn) triggerBtn.disabled = false;
    }
  }

  // ── Reports ───────────────────────────────────────────────────────
  async function resolveReport(id, status, triggerBtn) {
    if (triggerBtn) triggerBtn.disabled = true;
    try {
      var res = await FerumApi.mod.resolveReport(id, status);
      if (res.ok) {
        Ferum.showToast('report-toast', status === 'resolved' ? 'Report resolved.' : 'Report dismissed.');
        setTimeout(function () { location.reload(); }, 1000);
      } else {
        var b = await res.json().catch(function () { return {}; });
        Ferum.showToast('report-toast', (b.error && b.error.message) || 'Action failed.', true);
        if (triggerBtn) triggerBtn.disabled = false;
      }
    } catch (_) {
      Ferum.showToast('report-toast', Ferum.t('js-network-error'), true);
      if (triggerBtn) triggerBtn.disabled = false;
    }
  }

  // ── Threads ───────────────────────────────────────────────────────
  // `currentState` arrives as the raw data-current-state string and is parsed
  // per action: "true"/"false" for pin, the thread status for lock. Coercing it
  // to a boolean at the call site cannot work — the string "false" is truthy.
  async function modThread(action, threadId, currentState, triggerBtn) {
    if (triggerBtn) triggerBtn.disabled = true;
    try {
      var res = action === 'pin'
        ? await FerumApi.threads.pin(threadId, currentState !== 'true')
        : await FerumApi.threads.lock(threadId, currentState !== 'locked');
      if (res.ok) location.reload();
      else {
        var b = await res.json().catch(function () { return {}; });
        Ferum.toast((b.error && b.error.message) || 'Action failed.', true);
        if (triggerBtn) triggerBtn.disabled = false;
      }
    } catch (_) {
      Ferum.toast(Ferum.t('js-network-error'), true);
      if (triggerBtn) triggerBtn.disabled = false;
    }
  }

  // ── Users ─────────────────────────────────────────────────────────
  async function warnUser(userId, username, triggerBtn) {
    var reasonEl = document.getElementById('warn-reason-' + userId);
    var reason = reasonEl && reasonEl.value.trim();
    if (!reason) { Ferum.showFeedback('warn-feedback-' + userId, 'warning', 'Please enter a reason.'); return; }
    if (triggerBtn) triggerBtn.disabled = true;
    try {
      var res = await FerumApi.mod.warnUser(userId, reason);
      if (res.ok) {
        Ferum.showFeedback('warn-feedback-' + userId, 'success', 'Warning issued to ' + username + '.');
        if (reasonEl) reasonEl.value = '';
      } else {
        var b = await res.json().catch(function () { return {}; });
        Ferum.showFeedback('warn-feedback-' + userId, 'danger', (b.error && b.error.message) || 'Failed to warn user.');
      }
    } catch (_) { Ferum.showFeedback('warn-feedback-' + userId, 'danger', Ferum.t('js-network-error')); }
    if (triggerBtn) triggerBtn.disabled = false;
  }

  async function banUser(userId, username, triggerBtn) {
    var reasonEl = document.getElementById('ban-reason-' + userId);
    var untilEl  = document.getElementById('ban-until-' + userId);
    var reason   = reasonEl && reasonEl.value.trim();
    var until    = untilEl  && untilEl.value;
    if (!reason) { Ferum.showFeedback('ban-feedback-' + userId, 'warning', 'Please enter a reason.'); return; }
    if (!until)  { Ferum.showFeedback('ban-feedback-' + userId, 'warning', 'Please set a ban expiry date.'); return; }
    if (triggerBtn) triggerBtn.disabled = true;
    try {
      var res = await FerumApi.mod.banUser(userId, reason, new Date(until).toISOString());
      if (res.ok) {
        Ferum.showFeedback('ban-feedback-' + userId, 'success', username + ' has been banned.');
        if (reasonEl) reasonEl.value = '';
        if (untilEl)  untilEl.value  = '';
      } else {
        var b = await res.json().catch(function () { return {}; });
        Ferum.showFeedback('ban-feedback-' + userId, 'danger', (b.error && b.error.message) || 'Failed to ban user.');
      }
    } catch (_) { Ferum.showFeedback('ban-feedback-' + userId, 'danger', Ferum.t('js-network-error')); }
    if (triggerBtn) triggerBtn.disabled = false;
  }

  // ── Event delegation ─────────────────────────────────────────────
  document.addEventListener('click', function (e) {
    // Toast close buttons (plain data-action)
    var closeBtn = e.target.closest('[data-action="close-toast"]');
    if (closeBtn) { closeBtn.closest('.toast')?.classList.add('d-none'); return; }

    var btn = e.target.closest('[data-mod-action]');
    if (!btn) return;
    switch (btn.dataset.modAction) {
      case 'approve':        approvePost(btn.dataset.postId, btn); break;
      case 'reject':         rejectPost(btn.dataset.postId, btn); break;
      case 'resolve-report': resolveReport(btn.dataset.reportId, btn.dataset.status, btn); break;
      case 'mod-thread':     modThread(btn.dataset.action, btn.dataset.threadId, btn.dataset.currentState, btn); break;
      case 'warn-user':      warnUser(btn.dataset.userId, btn.dataset.username, btn); break;
      case 'ban-user':       banUser(btn.dataset.userId, btn.dataset.username, btn); break;
    }
  });
}());
