(function () {
  'use strict';

  window.thumbnailUpload = function (threadId, initialUrl) {
    return {
      threadId: threadId,
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
          var res = await FerumApi.threads.uploadThumbnail(this.threadId, fd);
          if (!res.ok) {
            var body = await res.json().catch(function () { return {}; });
            this.uploadError = (body.error && body.error.message) || Ferum.t('js-upload-failed');
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
        await FerumApi.threads.deleteThumbnail(this.threadId).catch(function () {});
      },
    };
  };

  // tagChipInput for edit page (max 5 tags, supports initial seed)
  window.tagChipInput = function (initialTags) {
    var seedNames = (initialTags || []).map(function (t) { return t.name || t; });
    return {
      tags: seedNames,
      current: '',
      add: function () {
        var t = this.current.trim().replace(/,+$/, '');
        if (t && !this.tags.includes(t) && this.tags.length < 5) this.tags.push(t);
        this.current = '';
      },
      checkComma: function (e) {
        if (e.key === ',') { e.preventDefault(); this.add(); }
      },
      remove: function (idx) {
        if (idx >= 0 && idx < this.tags.length) this.tags.splice(idx, 1);
      },
    };
  };

  var _editThreadSubmitting = false;

  document.getElementById('edit-thread-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    if (_editThreadSubmitting) return;
    var btn     = document.getElementById('submit-btn');
    var spinner = document.getElementById('btn-spinner');
    var errorEl = document.getElementById('form-error');
    // Thread slug is stored as a data attribute on the form element
    var threadSlug = e.currentTarget.dataset.threadSlug || '';
    var id         = document.getElementById('thread-id').value;

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

    try {
      var res = await FerumApi.threads.update(id, fd);
      if (res.ok) {
        window.location.href = '/forum/t/' + threadSlug;
      } else {
        var body = await res.json().catch(function () { return {}; });
        errorEl.textContent = (body.error && body.error.message) || Ferum.t('js-failed-save-changes');
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
