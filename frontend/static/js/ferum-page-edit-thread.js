(function () {
  'use strict';

  window.thumbnailUpload = function (threadSlug, initialUrl) {
    return {
      threadSlug: threadSlug,
      preview: initialUrl || null,
      uploadError: '',
      isDragging: false,
      uploading: false,
      onDrop: function (e) { var f = e.dataTransfer.files && e.dataTransfer.files[0]; if (f) this.handleFile(f); },
      onFileChange: function (e) { var f = e.target.files && e.target.files[0]; if (f) this.handleFile(f); },
      handleFile: function (file) {
        var ACCEPTED = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];
        this.uploadError = '';
        if (!ACCEPTED.includes(file.type)) { this.uploadError = Ferum.t('js-thumbnail-invalid-type'); return; }
        if (file.size > 10 * 1024 * 1024) { this.uploadError = Ferum.t('js-thumbnail-too-large'); return; }
        var reader = new FileReader();
        var self = this;
        reader.onload = function (ev) { self.preview = ev.target.result; };
        reader.readAsDataURL(file);
        this.uploadToServer(file);
      },
      uploadToServer: async function (file) {
        this.uploading = true;
        var fd = new FormData();
        fd.append('file', file);
        try {
          var res = await FerumApi.threads.uploadThumbnail(this.threadSlug, fd);
          if (!res.ok) {
            var body = await res.json().catch(function () { return {}; });
            this.uploadError = Ferum.errorMessage(body) || Ferum.t('js-upload-failed');
            this.preview = null;
          }
        } catch (_) {
          this.uploadError = Ferum.t('js-network-error');
          this.preview = null;
        } finally {
          this.uploading = false;
        }
      },
      removeThumbnail: async function () {
        this.preview = null;
        this.uploadError = '';
        await FerumApi.threads.deleteThumbnail(this.threadSlug).catch(function () {});
      },
    };
  };

  // tagChipInput comes from ferum-api.js — one implementation, one tag limit.
  // The copy that used to live here was identical to it.

  var _editThreadSubmitting = false;

  document.getElementById('edit-thread-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    if (_editThreadSubmitting) return;
    var btn     = document.getElementById('submit-btn');
    var spinner = document.getElementById('btn-spinner');
    var errorEl = document.getElementById('form-error');
    // Thread slug is stored as a data attribute on the form element
    var threadSlug = e.currentTarget.dataset.threadSlug || '';

    _editThreadSubmitting = true;
    btn.disabled = true;
    spinner.classList.remove('d-none');
    errorEl.classList.add('d-none');

    var composer   = document.getElementById('composer');
    var content_md = (composer && composer.getContent) ? (composer.getContent() || '') : (document.getElementById('content')?.value || '');

    var tags = [];
    try { tags = JSON.parse(document.getElementById('tags-json')?.value || '[]'); } catch (_) {}

    var fd = new FormData();
    fd.append('title', document.getElementById('title').value);
    fd.append('content_md', content_md);
    if (tags.length > 0) fd.append('tags', tags.join(','));

    // A category change is a separate, more privileged endpoint. The select is
    // only enabled when the server said `thread.move` holds here.
    var catSelect = document.getElementById('category');
    var movedTo = (catSelect && !catSelect.disabled && catSelect.value !== catSelect.dataset.originalCategory)
      ? catSelect.value
      : null;

    try {
      var res = await FerumApi.threads.update(threadSlug, fd);
      if (res.ok) {
        // Ordered after the edit on purpose: both calls are idempotent, so a
        // failed move leaves the title/content saved and a retry re-runs only
        // what is still outstanding.
        if (movedTo) {
          var moveRes = await FerumApi.threads.move(threadSlug, movedTo);
          if (!moveRes.ok) {
            var moveBody = await moveRes.json().catch(function () { return {}; });
            errorEl.textContent = Ferum.errorMessage(moveBody) || Ferum.t('js-failed-move-thread');
            errorEl.classList.remove('d-none');
            btn.disabled = false;
            spinner.classList.add('d-none');
            _editThreadSubmitting = false;
            return;
          }
          catSelect.dataset.originalCategory = movedTo;
        }
        window.location.href = '/forum/t/' + threadSlug;
      } else {
        var body = await res.json().catch(function () { return {}; });
        errorEl.textContent = Ferum.errorMessage(body) || Ferum.t('js-failed-save-changes');
        errorEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
        _editThreadSubmitting = false;
      }
    } catch (_) {
      errorEl.textContent = Ferum.t('js-network-error');
      errorEl.classList.remove('d-none');
      btn.disabled = false;
      spinner.classList.add('d-none');
      _editThreadSubmitting = false;
    }
  });
}());
