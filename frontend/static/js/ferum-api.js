/**
 * FerumApi — centralized HTTP client for template scripts.
 *
 * All fetch() calls go through request() below. To swap the HTTP library,
 * change only that one function.
 *
 * Usage: FerumApi.auth.login(email, password).then(res => { ... })
 */
(function () {
  'use strict';

  // ── Base HTTP client ─────────────────────────────────────────────────────

  async function request(url, init) {
    var opts = Object.assign({ method: 'GET' }, init);
    var headers = Object.assign({}, opts.headers || {});
    // Auto-set Content-Type only for serialized JSON bodies.
    if (typeof opts.body === 'string' && !headers['Content-Type']) {
      headers['Content-Type'] = 'application/json';
    }
    opts.headers = headers;
    return fetch(url, opts);
  }

  function get(url) {
    return request(url, { method: 'GET' });
  }

  function post(url, body) {
    return request(url, { method: 'POST', body: body != null ? JSON.stringify(body) : undefined });
  }

  function postForm(url, formData) {
    // Let the browser set multipart/form-data with boundary automatically.
    return request(url, { method: 'POST', body: formData });
  }

  function put(url, body) {
    return request(url, { method: 'PUT', body: body != null ? JSON.stringify(body) : undefined });
  }

  function patch(url, body) {
    return request(url, { method: 'PATCH', body: body != null ? JSON.stringify(body) : undefined });
  }

  function del(url) {
    return request(url, { method: 'DELETE' });
  }

  // Verb chosen at runtime, for call sites that decide create-vs-update from
  // state ("PATCH if editing, else POST"). Without it each such caller writes
  // its own switch, which is what the catalogue screens did.
  function send(method, url, body) {
    switch (String(method).toUpperCase()) {
      case 'POST':   return post(url, body);
      case 'PUT':    return put(url, body);
      case 'PATCH':  return patch(url, body);
      case 'DELETE': return del(url);
      default:       return get(url);
    }
  }

  // ── Domain services ──────────────────────────────────────────────────────

  window.FerumApi = {

    // The verbs themselves, for endpoint families that have no domain group
    // below — currently the admin catalogue screens (products, brands,
    // materials, product-categories), which are ~25 CRUD endpoints used by two
    // files and nothing else.
    //
    // They are exposed because the alternative was worse, not because the
    // domain groups are optional. ferum-admin-products.js and
    // ferum-product-manage.js each carried a private getJSON/sendJSON pair --
    // sendJSON byte-identical between them -- and 15 bare fetch() calls, which
    // made the promise at the top of this file ("all fetch() calls go through
    // request()") false. Routing them here restores it without inventing 25
    // named methods that would each be called once.
    //
    // Prefer a domain group for anything a second surface starts using.
    http: {
      get: get,
      post: post,
      postForm: postForm,
      put: put,
      patch: patch,
      del: del,
      send: send,
    },

    auth: {
      login: function (email, password) {
        return post('/api/auth/sessions', { email: email, password: password });
      },
      logout: function () {
        return del('/api/auth/sessions');
      },
      register: function (data) {
        return post('/api/auth/registrations', data);
      },
      forgotPassword: function (email) {
        return post('/api/auth/password-resets', { email: email });
      },
      resendVerification: function (email) {
        return post('/api/auth/verify-email/resend', { email: email });
      },
      resetPassword: function (token, password) {
        return patch('/api/auth/password-resets/' + token, { new_password: password });
      },
    },

    // Threads are addressed by SLUG on every /api/threads/* route — the same
    // identifier the reader sees in /forum/t/{slug}. It used to be slug on some
    // verbs and UUID on others, on the same paths.
    threads: {
      create: function (formData) {
        return postForm('/api/threads', formData);
      },
      update: function (slug, formData) {
        return request('/api/threads/' + slug, { method: 'PATCH', body: formData });
      },
      delete: function (slug) {
        return del('/api/threads/' + slug);
      },
      pin: function (slug, pinned) {
        return patch('/api/threads/' + slug + '/pin', { pinned: pinned });
      },
      lock: function (slug, locked) {
        return patch('/api/threads/' + slug + '/lock', { locked: locked });
      },
      move: function (slug, categoryId) {
        return patch('/api/threads/' + slug + '/move', { category_id: categoryId });
      },
      solve: function (slug, bestAnswerId) {
        return patch('/api/threads/' + slug + '/solve', { best_answer_id: bestAnswerId });
      },
      uploadThumbnail: function (slug, formData) {
        return postForm('/api/threads/' + slug + '/thumbnail', formData);
      },
      deleteThumbnail: function (slug) {
        return del('/api/threads/' + slug + '/thumbnail');
      },
    },

    posts: {
      create: function (threadSlug, contentMd) {
        return post('/api/threads/' + threadSlug + '/posts', { content_md: contentMd });
      },
      update: function (id, contentMd) {
        return patch('/api/posts/' + id, { content_md: contentMd });
      },
      delete: function (id) {
        return del('/api/posts/' + id);
      },
      report: function (postId, reason) {
        return post('/api/reports', { post_id: postId, reason: reason });
      },
    },

    users: {
      updateProfile: function (data) {
        return patch('/api/users/me', data);
      },
      changePassword: function (currentPassword, newPassword) {
        return patch('/api/users/me/password', { current_password: currentPassword, new_password: newPassword });
      },
      uploadAvatar: function (formData) {
        return postForm('/api/users/me/avatar', formData);
      },
      removeAvatar: function () {
        return del('/api/users/me/avatar');
      },
      uploadCover: function (formData) {
        return postForm('/api/users/me/cover', formData);
      },
      removeCover: function () {
        return del('/api/users/me/cover');
      },
      getPreferences: function () {
        return get('/api/users/me/preferences');
      },
      updatePreferences: function (prefs) {
        return put('/api/users/me/preferences', prefs);
      },
      getFollowStatus: function (userId) {
        return get('/api/users/' + userId + '/follow-status');
      },
      follow: function (userId) {
        return post('/api/users/' + userId + '/follow');
      },
      unfollow: function (userId) {
        return del('/api/users/' + userId + '/follow');
      },
      // Generic GET for dynamic tab URLs (posts, followers, following, etc.)
      fetchUrl: function (url) {
        return get(url);
      },
    },

    reports: {
      mine: function (page) {
        return get('/api/reports/mine?page=' + (page || 1));
      },
    },

    notifications: {
      markRead: function (id) {
        return patch('/api/notifications/' + id + '/read');
      },
      markAllRead: function () {
        return patch('/api/notifications/read-all');
      },
    },

    mod: {
      warnUser: function (userId, reason) {
        return post('/api/mod/users/' + userId + '/warn', { reason: reason });
      },
      banUser: function (userId, reason, until) {
        return post('/api/mod/users/' + userId + '/ban', { reason: reason, until: until });
      },
      resolveReport: function (id, status) {
        return patch('/api/mod/reports/' + id, { status: status });
      },
      approvePost: function (postId) {
        return post('/api/mod/queue/' + postId + '/approve');
      },
      rejectPost: function (postId) {
        return del('/api/mod/queue/' + postId + '/reject');
      },
    },

    admin: {
      getConfig: function () {
        return get('/api/admin/config');
      },
      saveConfig: function (body) {
        return put('/api/admin/config', body);
      },
      uploadBranding: function (type, formData) {
        return postForm('/api/admin/config/' + type, formData);
      },
      removeBranding: function (type) {
        return del('/api/admin/config/' + type);
      },
      // No body: the recipient is always the acting admin's own address, decided
      // server-side so this cannot be turned into an open relay.
      testEmail: function () {
        return post('/api/admin/email/test', {});
      },
      getWebhooks: function () {
        return get('/api/admin/webhooks');
      },
      createWebhook: function (data) {
        return post('/api/admin/webhooks', data);
      },
      toggleWebhook: function (id, isActive) {
        return patch('/api/admin/webhooks/' + id, { is_active: isActive });
      },
      updateWebhook: function (id, data) {
        return patch('/api/admin/webhooks/' + id, data);
      },
      testWebhook: function (id) {
        return post('/api/admin/webhooks/' + id + '/test', {});
      },
      deleteWebhook: function (id) {
        return del('/api/admin/webhooks/' + id);
      },
      createRole: function (data) {
        return post('/api/admin/roles', data);
      },
      updateRole: function (id, data) {
        return patch('/api/admin/roles/' + id, data);
      },
      deleteRole: function (id) {
        return del('/api/admin/roles/' + id);
      },
      editUser: function (id, data) {
        return patch('/api/admin/users/' + id, data);
      },
      setTrustLevel: function (id, level) {
        return patch('/api/admin/users/' + id + '/trust-level', { trust_level: level });
      },
      unlockUser: function (id) {
        return post('/api/admin/users/' + id + '/unlock', {});
      },
      verifyUserEmail: function (id) {
        return post('/api/admin/users/' + id + '/verify-email', {});
      },
      banUser: function (id, reason, until) {
        return post('/api/admin/users/' + id + '/ban', { reason: reason, banned_until: until || null });
      },
      unbanUser: function (id) {
        return del('/api/admin/users/' + id + '/ban');
      },
      assignRole: function (userId, roleId) {
        return post('/api/admin/users/' + userId + '/roles', { role_id: roleId });
      },
      revokeRole: function (userId, roleId) {
        return del('/api/admin/users/' + userId + '/roles/' + roleId);
      },
      listModerators: function (categoryId) {
        return get('/api/admin/categories/' + categoryId + '/moderators');
      },
      assignModerator: function (categoryId, userId) {
        return post('/api/admin/categories/' + categoryId + '/moderators', { user_id: userId });
      },
      revokeModerator: function (categoryId, userId) {
        return del('/api/admin/categories/' + categoryId + '/moderators/' + userId);
      },
      createCategory: function (data) {
        return post('/api/admin/categories', data);
      },
      updateCategory: function (id, data) {
        return patch('/api/admin/categories/' + id, data);
      },
      deleteCategory: function (id) {
        return del('/api/admin/categories/' + id);
      },
    },

    categories: {
      list: function () {
        return get('/api/categories');
      },
    },

    // ── Plugin-facing helpers ────────────────────────────────────────────
    // Available to any plugin UI-slot script once ferum-api.js has loaded
    // (it's always injected before plugin bundles — see base.html). Callers
    // still get a raw fetch() Response back, same as every other FerumApi
    // method, so existing .ok / .json() handling works unmodified.
    plugins: {
      // POST /api/plugins/:slug/rpc/:action — the generic inbound entry point
      // for a Script-tier plugin's own mini-API (e.g. chatbox send/get message).
      rpc: function (slug, action, payload) {
        return post('/api/plugins/' + encodeURIComponent(slug) + '/rpc/' + encodeURIComponent(action), payload || {});
      },
      // POST /api/plugins/:slug/media — multipart upload into the plugin's own
      // CAS namespace; requires granted_capabilities.media on the plugin.
      uploadMedia: function (slug, formData) {
        return postForm('/api/plugins/' + encodeURIComponent(slug) + '/media', formData);
      },
    },

    setup: {
      run: function (data) {
        return post('/api/setup/run', data);
      },
    },
  };

  // ── Shared Alpine.js data factory — tag chip input ─────────────────────
  // Exposed globally so Alpine can resolve it via x-data="tagChipInput()"
  // and x-data="tagChipInput([...existing tags...])"
  //
  // The one implementation. The compose and edit pages each carried their own
  // near-identical copy that shadowed this one at load time — which is the only
  // reason the wrong limit here was not reaching users, and would have stopped
  // being true the moment a third page used the shared factory as intended.
  //
  // MAX_TAGS must match `MAX_TAGS_PER_THREAD` in the backend's constants.rs.
  // This said 10 while the server takes the first 5 and drops the rest without
  // a word, so a user could add ten tags, watch them all appear as chips, save,
  // and find five of them gone with nothing having reported a problem.
  var MAX_TAGS = 5;

  window.tagChipInput = function (initialTags) {
    var seedNames = (initialTags || []).map(function (t) {
      return typeof t === 'string' ? t : (t.name || t);
    });
    return {
      tags: seedNames,
      maxTags: MAX_TAGS,
      current: '',
      add: function () {
        var t = this.current.trim().replace(/,+$/, '');
        if (t && !this.tags.includes(t) && this.tags.length < MAX_TAGS) this.tags.push(t);
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


}());
