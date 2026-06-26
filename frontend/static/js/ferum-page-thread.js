(function () {
  'use strict';

  // ── Read page-level data from hidden element ─────────────────────
  var pd = document.getElementById('ferum-page-data');
  if (!pd) return;

  var THREAD_ID  = pd.dataset.threadId  || '';
  var PAGE_OFFSET = parseInt(pd.dataset.pageOffset || '0', 10);
  var CURRENT_USER = pd.dataset.currentUserId ? {
    id:           pd.dataset.currentUserId,
    username:     pd.dataset.currentUserUsername || '',
    display_name: pd.dataset.currentUserDisplay  || '',
    avatar_url:   pd.dataset.currentUserAvatar   || '',
  } : undefined;

  // ── Generic confirmation modal ───────────────────────────────────
  var _confirmResolve = null;

  function showConfirm(title, body, okLabel, okVariant) {
    okLabel   = okLabel   || 'Confirm';
    okVariant = okVariant || 'danger';
    return new Promise(function (resolve) {
      document.getElementById('confirmModalTitle').textContent = title;
      document.getElementById('confirmModalBody').textContent  = body;
      var okBtn = document.getElementById('confirmModalOk');
      okBtn.textContent = okLabel;
      okBtn.className   = 'btn btn-' + okVariant + ' btn-sm';
      _confirmResolve   = resolve;
      bootstrap.Modal.getOrCreateInstance(document.getElementById('confirmModal')).show();
    });
  }

  document.getElementById('confirmModalOk').addEventListener('click', function () {
    bootstrap.Modal.getInstance(document.getElementById('confirmModal'))?.hide();
    if (_confirmResolve) { _confirmResolve(true); _confirmResolve = null; }
  });
  document.getElementById('confirmModal').addEventListener('hidden.bs.modal', function () {
    if (_confirmResolve) { _confirmResolve(false); _confirmResolve = null; }
  });

  // ── Toast notifications ──────────────────────────────────────────
  // Delegates to Ferum.toast (ferum-utils.js) — single implementation site.
  function showToast(message, type) {
    Ferum.toast(message, type !== 'success');
  }

  // ── HTML escape helper ───────────────────────────────────────────
  function escHtml(s) {
    return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  // ── Build a reply card in JS ─────────────────────────────────────
  function buildReplyCard(post, postNum) {
    var u = CURRENT_USER || {};
    var avatarHtml = u.avatar_url
      ? '<img src="' + escHtml(u.avatar_url) + '" class="fr-avatar fr-avatar--38" alt="' + escHtml(u.display_name) + '" loading="lazy">'
      : '<div class="fr-avatar--placeholder fr-avatar--38" aria-label="' + escHtml(u.display_name) + '">' + (u.display_name || '?')[0].toUpperCase() + '</div>';
    var dateStr = new Date().toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
    return '<div class="card mb-3" id="post-' + escHtml(post.id) + '">' +
      '<div class="card-body">' +
        '<div class="d-flex justify-content-between align-items-start mb-3">' +
          '<div class="d-flex align-items-center gap-2">' +
            avatarHtml +
            '<div>' +
              '<div class="d-flex align-items-center gap-1">' +
                '<a href="/u/' + escHtml(u.username) + '" class="fw-semibold text-body text-decoration-none">' + escHtml(u.display_name) + '</a>' +
              '</div>' +
              '<div class="d-flex align-items-center gap-2 mt-1">' +
                '<time class="small text-muted">' + dateStr + '</time>' +
              '</div>' +
            '</div>' +
          '</div>' +
          '<a href="#post-' + escHtml(post.id) + '" class="small text-muted text-decoration-none fr-post-num">#' + postNum + '</a>' +
        '</div>' +
        '<div id="post-content-' + escHtml(post.id) + '" class="post-content">' + (post.content_html || '') + '</div>' +
        '<div id="post-edit-' + escHtml(post.id) + '" class="d-none mt-2"></div>' +
        '<div class="mt-3 pt-2 border-top">' +
          '<ferum-reaction-bar post-id="' + escHtml(post.id) + '" reactions=\'[]\' user-id="' + escHtml(u.id || '') + '"></ferum-reaction-bar>' +
        '</div>' +
      '</div>' +
    '</div>';
  }

  // ── Report modal pre-population ──────────────────────────────────
  var reportModal = document.getElementById('reportModal');
  if (reportModal) {
    reportModal.addEventListener('show.bs.modal', function (e) {
      var btn = e.relatedTarget;
      if (btn && btn.dataset.postId) {
        document.getElementById('report-post-id').value          = btn.dataset.postId;
        document.getElementById('report-author-name').textContent = btn.dataset.author || '';
        document.getElementById('report-reason').value           = '';
        document.getElementById('report-feedback').classList.add('d-none');
        var submitBtn = document.getElementById('report-submit-btn');
        submitBtn.disabled = false;
        var spinner = document.getElementById('report-spinner');
        if (spinner) spinner.classList.add('d-none');
      }
    });
  }

  // ── Post actions ─────────────────────────────────────────────────
  async function deletePost(postId) {
    var ok = await showConfirm('Delete Post', 'Delete this post? This cannot be undone.', 'Delete');
    if (!ok) return;
    var res = await FerumApi.posts.delete(postId);
    if (res.ok) {
      var el = document.getElementById('post-' + postId);
      if (el) el.innerHTML = '<div class="card-body text-muted small py-2"><i class="fa-solid fa-trash me-1"></i>Post deleted.</div>';
    } else {
      var body = await res.json().catch(function () { return {}; });
      showToast((body.error && body.error.message) || 'Failed to delete post.');
    }
  }

  function openEditPost(postId) {
    document.getElementById('post-content-' + postId)?.classList.add('d-none');
    document.getElementById('post-edit-' + postId)?.classList.remove('d-none');
    document.getElementById('post-edit-composer-' + postId)?.focus?.();
  }

  function cancelEdit(postId) {
    document.getElementById('post-content-' + postId)?.classList.remove('d-none');
    document.getElementById('post-edit-' + postId)?.classList.add('d-none');
  }

  async function saveEdit(postId) {
    var composer = document.getElementById('post-edit-composer-' + postId);
    var content  = (composer?.getContent?.() ?? '').trim();
    if (!content) return;

    var saveBtn = document.querySelector('[data-action="save-edit"][data-post-id="' + postId + '"]');
    if (saveBtn) saveBtn.disabled = true;

    try {
      var res = await FerumApi.posts.update(postId, content);
      if (res.ok) {
        var body      = await res.json().catch(function () { return {}; });
        var html      = body.data && body.data.content_html;
        var contentEl = document.getElementById('post-content-' + postId);
        if (html && contentEl) contentEl.innerHTML = html;
        cancelEdit(postId);
        var postCard = document.getElementById('post-' + postId);
        if (postCard && !postCard.querySelector('.fr-post-edited')) {
          var timeEl = postCard.querySelector('time.small.text-muted');
          if (timeEl) {
            var span = document.createElement('span');
            span.className = 'small text-muted fr-post-edited';
            span.innerHTML = ' · <i class="fa-solid fa-pen fa-xs me-1 opacity-50"></i>edited';
            timeEl.insertAdjacentElement('afterend', span);
          }
        }
      } else {
        var body = await res.json().catch(function () { return {}; });
        showToast((body.error && body.error.message) || 'Failed to save changes.');
        if (saveBtn) saveBtn.disabled = false;
      }
    } catch (_) {
      showToast('Network error. Please try again.');
      if (saveBtn) saveBtn.disabled = false;
    }
  }

  async function submitReport() {
    var postId = document.getElementById('report-post-id').value;
    var reason = document.getElementById('report-reason').value.trim();
    var fb     = document.getElementById('report-feedback');
    if (!reason) {
      fb.className = 'alert alert-warning';
      fb.textContent = 'Please provide a reason.';
      fb.classList.remove('d-none');
      return;
    }
    var btn     = document.getElementById('report-submit-btn');
    var spinner = document.getElementById('report-spinner');
    btn.disabled = true;
    if (spinner) spinner.classList.remove('d-none');
    try {
      var res = await FerumApi.posts.report(postId, reason);
      if (res.ok) {
        fb.className   = 'alert alert-success';
        fb.textContent = 'Report submitted. Thank you.';
        fb.classList.remove('d-none');
        setTimeout(function () {
          bootstrap.Modal.getInstance(document.getElementById('reportModal'))?.hide();
        }, 1500);
      } else {
        var body = await res.json().catch(function () { return {}; });
        fb.className   = 'alert alert-danger';
        fb.textContent = (body.error && body.error.message) || 'Failed to submit report.';
        fb.classList.remove('d-none');
        btn.disabled   = false;
        if (spinner) spinner.classList.add('d-none');
      }
    } catch (_) {
      fb.className   = 'alert alert-danger';
      fb.textContent = 'Network error. Please try again.';
      fb.classList.remove('d-none');
      btn.disabled   = false;
      if (spinner) spinner.classList.add('d-none');
    }
  }

  async function submitReply() {
    var composer = document.getElementById('reply-composer');
    var content  = (composer?.getContent?.() ?? '').trim();
    var feedback = document.getElementById('reply-feedback');
    var btn      = document.getElementById('reply-submit-btn');
    var spinner  = document.getElementById('reply-spinner');

    if (!content) {
      feedback.className = 'alert alert-warning mt-2';
      feedback.textContent = 'Please write something before posting.';
      feedback.classList.remove('d-none');
      return;
    }

    btn.disabled = true;
    spinner.classList.remove('d-none');
    feedback.classList.add('d-none');

    try {
      var res = await FerumApi.posts.create(THREAD_ID, content);
      if (res.ok) {
        var body = await res.json().catch(function () { return {}; });
        var post = body.data;
        if (post && post.id) {
          var existingCards = document.querySelectorAll('.card[id^="post-"]').length;
          var postNum       = PAGE_OFFSET + existingCards + 1;
          var replyBox      = document.getElementById('reply');
          replyBox.insertAdjacentHTML('beforebegin', buildReplyCard(post, postNum));
          if (customElements.upgrade) customElements.upgrade(document.getElementById('post-' + post.id));
          document.getElementById('post-' + post.id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
          var countEl = document.getElementById('fr-reply-count');
          if (countEl) countEl.textContent = parseInt(countEl.textContent || '0') + 1;
          if (composer && composer.reset) composer.reset();
          feedback.classList.add('d-none');
        } else {
          window.location.reload();
        }
        btn.disabled = false;
        spinner.classList.add('d-none');
      } else {
        var body = await res.json().catch(function () { return {}; });
        feedback.className   = 'alert alert-danger mt-2';
        feedback.textContent = (body.error && body.error.message) || 'Failed to post reply.';
        feedback.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
      }
    } catch (_) {
      feedback.className   = 'alert alert-danger mt-2';
      feedback.textContent = 'Network error. Please try again.';
      feedback.classList.remove('d-none');
      btn.disabled = false;
      spinner.classList.add('d-none');
    }
  }

  // ── Thread actions ───────────────────────────────────────────────
  async function markBestAnswer(threadId, postId) {
    var ok = await showConfirm('Mark Best Answer', 'Mark this post as the best answer?', 'Mark as best', 'success');
    if (!ok) return;
    var res = await FerumApi.threads.solve(threadId, postId);
    if (res.ok) {
      window.location.reload();
    } else {
      var body = await res.json().catch(function () { return {}; });
      showToast((body.error && body.error.message) || 'Failed to mark best answer.');
    }
  }

  async function moveThread(threadId) {
    var categoryId = document.getElementById('move-category-id').value;
    var fb         = document.getElementById('move-feedback');
    if (!categoryId) {
      fb.className   = 'alert alert-warning mt-2';
      fb.textContent = 'Please select a target category.';
      fb.classList.remove('d-none');
      return;
    }
    var btn     = document.getElementById('move-submit-btn');
    var spinner = document.getElementById('move-spinner');
    btn.disabled = true;
    if (spinner) spinner.classList.remove('d-none');
    try {
      var res = await FerumApi.threads.move(threadId, categoryId);
      if (res.ok) {
        window.location.reload();
      } else {
        var body = await res.json().catch(function () { return {}; });
        fb.className   = 'alert alert-danger mt-2';
        fb.textContent = (body.error && body.error.message) || 'Failed to move thread.';
        fb.classList.remove('d-none');
        btn.disabled   = false;
        if (spinner) spinner.classList.add('d-none');
      }
    } catch (_) {
      fb.className   = 'alert alert-danger mt-2';
      fb.textContent = 'Network error. Please try again.';
      fb.classList.remove('d-none');
      btn.disabled   = false;
      if (spinner) spinner.classList.add('d-none');
    }
  }

  async function deleteThread(threadId, categorySlug) {
    var ok = await showConfirm('Delete Thread', 'Delete this entire thread? This cannot be undone.', 'Delete thread');
    if (!ok) return;
    var res = await FerumApi.threads.delete(threadId);
    if (res.ok) {
      window.location.href = categorySlug ? '/forum/' + categorySlug : '/forum';
    } else {
      var body = await res.json().catch(function () { return {}; });
      showToast((body.error && body.error.message) || 'Failed to delete thread.');
    }
  }

  async function modAction(action, threadId, currentState, triggerBtn) {
    if (triggerBtn) triggerBtn.disabled = true;
    var res = action === 'pin'
      ? await FerumApi.threads.pin(threadId, !currentState)
      : await FerumApi.threads.lock(threadId, !currentState);
    if (res.ok) {
      window.location.reload();
    } else {
      var body = await res.json().catch(function () { return {}; });
      showToast((body.error && body.error.message) || 'Failed to ' + action + ' thread.');
      if (triggerBtn) triggerBtn.disabled = false;
    }
  }

  // ── Event delegation ─────────────────────────────────────────────
  document.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-action]');
    if (!btn) return;
    switch (btn.dataset.action) {
      case 'delete-thread':  deleteThread(btn.dataset.threadId, btn.dataset.categorySlug); break;
      case 'mod-thread':     modAction(btn.dataset.modAction, btn.dataset.threadId, btn.dataset.currentState === 'true', btn); break;
      case 'edit-post':      openEditPost(btn.dataset.postId); break;
      case 'delete-post':    deletePost(btn.dataset.postId); break;
      case 'best-answer':    markBestAnswer(btn.dataset.threadId, btn.dataset.postId); break;
      case 'save-edit':      saveEdit(btn.dataset.postId); break;
      case 'cancel-edit':    cancelEdit(btn.dataset.postId); break;
      case 'submit-reply':   submitReply(); break;
      case 'cancel-reply':
        document.getElementById('reply-composer')?.reset?.();
        document.getElementById('reply-feedback')?.classList.add('d-none');
        break;
      case 'submit-report':  submitReport(); break;
      case 'move-thread':    moveThread(btn.dataset.threadId); break;
    }
  });
}());
