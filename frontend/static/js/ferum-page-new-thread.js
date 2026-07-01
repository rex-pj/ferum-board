(function () {
  'use strict';

  var _thumbState = null;

  window.newThreadThumbnail = function () {
    var state = {
      file: null,
      preview: null,
      thumbError: '',
      isDragging: false,
      onDrop: function (e) { var f = e.dataTransfer.files && e.dataTransfer.files[0]; if (f) this.handleFile(f); },
      onFileChange: function (e) { var f = e.target.files && e.target.files[0]; if (f) this.handleFile(f); },
      handleFile: function (f) {
        var ACCEPTED = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];
        this.thumbError = '';
        if (!ACCEPTED.includes(f.type)) { this.thumbError = 'Thumbnail must be a JPEG, PNG, GIF, or WebP image.'; return; }
        if (f.size > 10 * 1024 * 1024) { this.thumbError = 'Thumbnail must be under 10 MB.'; return; }
        this.file = f;
        var reader = new FileReader();
        var self = this;
        reader.onload = function (ev) { self.preview = ev.target.result; };
        reader.readAsDataURL(f);
      },
      remove: function () { this.file = null; this.preview = null; this.thumbError = ''; },
      uploadTo: async function (threadId) {
        if (!this.file) return;
        var fd = new FormData();
        fd.append('thumbnail', this.file);
        await FerumApi.threads.uploadThumbnail(threadId, fd);
      },
    };
    _thumbState = state;
    return state;
  };

  // tagChipInput used only on this page (max 5 tags, no initial seed)
  window.tagChipInput = function () {
    return {
      tags: [],
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

  var _newThreadSubmitting = false;

  document.getElementById('new-thread-form')?.addEventListener('submit', async function (e) {
    e.preventDefault();
    if (_newThreadSubmitting) return;
    var btn     = document.getElementById('submit-btn');
    var spinner = document.getElementById('btn-spinner');
    var errorEl = document.getElementById('form-error');

    _newThreadSubmitting = true;
    btn.disabled = true;
    spinner.classList.remove('d-none');
    errorEl.classList.add('d-none');

    var composer   = document.getElementById('composer');
    var content_md = (composer && composer.getContent) ? (composer.getContent() || '') : (document.getElementById('content')?.value || '');

    var tags = [];
    try { tags = JSON.parse(document.getElementById('tags-json')?.value || '[]'); } catch (_) {}

    var fd = new FormData();
    fd.append('category_id', document.getElementById('category').value);
    fd.append('title', document.getElementById('title').value);
    fd.append('content_md', content_md);
    if (tags.length > 0) fd.append('tags', tags.join(','));
    if (_thumbState && _thumbState.file) fd.append('thumbnail', _thumbState.file);

    try {
      var res = await FerumApi.threads.create(fd);
      if (res.ok) {
        var data = await res.json();
        var slug = (data.data && data.data.thread && data.data.thread.slug) || '';
        window.location.href = '/forum/t/' + slug;
      } else {
        var body = await res.json().catch(function () { return {}; });
        errorEl.textContent = (body.error && body.error.message) || 'Failed to post thread.';
        errorEl.classList.remove('d-none');
        btn.disabled = false;
        spinner.classList.add('d-none');
        _newThreadSubmitting = false;
      }
    } catch (_) {
      errorEl.textContent = 'Network error. Please try again.';
      errorEl.classList.remove('d-none');
      btn.disabled = false;
      spinner.classList.add('d-none');
      _newThreadSubmitting = false;
    }
  });
}());
