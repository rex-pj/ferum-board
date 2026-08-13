(function () {
  'use strict';

  // ── Admin sidebar toggle (mobile) ─────────────────────────────────
  document.querySelector('[data-action="toggle-admin-sidebar"]')?.addEventListener('click', function () {
    document.getElementById('adminSidebar')?.classList.toggle('show');
  });

  // ── Reporting timezone picker ─────────────────────────────────────
  // Same control and same filler as the one on /account — the value is the same
  // kind of thing, so it should not be a different widget in two places.
  Ferum.fillTimezoneSelect(document.getElementById('cfg-reporting_timezone'));

  // ── Primary color picker sync ─────────────────────────────────────
  (function () {
    var picker = document.getElementById('cfg-primary_color');
    var hex    = document.getElementById('primary-color-hex');
    if (!picker || !hex) return;
    picker.addEventListener('input', function () { hex.value = picker.value; });
    hex.addEventListener('input', function () {
      if (/^#[0-9a-fA-F]{6}$/.test(hex.value)) picker.value = hex.value;
    });
    hex.addEventListener('blur', function () {
      if (!/^#[0-9a-fA-F]{6}$/.test(hex.value)) hex.value = picker.value;
    });
  })();

  // ── Settings page ─────────────────────────────────────────────────
  function showSettingsAlert(msg, type) {
    var el  = document.getElementById('settings-alert');
    var txt = document.getElementById('settings-alert-msg');
    if (!el || !txt) return;
    el.className = 'alert alert-' + type + ' alert-dismissible fade show mb-4';
    txt.textContent = msg;
    el.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    if (type === 'success') {
      setTimeout(function () {
        var inst = window.bootstrap && bootstrap.Alert.getOrCreateInstance(el);
        if (inst) inst.close();
      }, 3500);
    }
  }

  function settingsApiError(r) {
    r.clone().json()
      .then(function (d) {
        showSettingsAlert(Ferum.errorMessage(d) || 'An error occurred.', 'danger');
      })
      .catch(function () {
        showSettingsAlert('An error occurred (HTTP ' + r.status + ').', 'danger');
      });
  }

  window.saveSettings = function (btn) {
    // Collect every cfg-* field across all tabs dynamically so new settings
    // added to any tab are included automatically.
    var body = {};
    document.querySelectorAll('[id^="cfg-"]').forEach(function (el) {
      // A disabled field is not submitted, matching plain HTML form semantics.
      // The Email tab disables the SMTP block when RESEND_API_KEY wins, and
      // without this those values would still be POSTed on every save — which
      // marks SMTP as "touched" and makes the server rebuild a transport nothing
      // is using, logging the provider-precedence warning each time.
      if (el.disabled) return;
      var key = el.id.replace(/^cfg-/, '');
      if (el.type === 'checkbox') {
        body[key] = el.checked ? 'true' : 'false';
      } else {
        var val = (el.value || '').trim();
        // Never overwrite an existing smtp_pass with an empty string —
        // leave the field blank to mean "keep current password".
        if (key === 'smtp_pass' && val === '') return;
        body[key] = val;
      }
    });

    if (!body.site_name) {
      showSettingsAlert('Site name is required. Please fill it in on the General tab.', 'danger');
      return;
    }

    btn.disabled = true;
    // Resolve the spinner element: prefer one inside the same tab-pane as the
    // button, fall back to the global #save-spinner on the General tab.
    var pane    = btn.closest('.tab-pane');
    var spinner = (pane && pane.querySelector('[id^="save-spinner"]')) ||
                  document.getElementById('save-spinner');
    spinner?.classList.remove('d-none');

    FerumApi.admin.saveConfig(body).then(function (r) {
      if (r.ok) showSettingsAlert('Settings saved successfully.', 'success');
      else settingsApiError(r);
    }).catch(function () {
      showSettingsAlert('Network error — settings not saved.', 'danger');
    }).finally(function () {
      btn.disabled = false;
      spinner?.classList.add('d-none');
    });
  };

  // Sends a real message through whichever provider is active, to the acting
  // admin's own address. Modelled on testWebhook: a failed delivery comes back as
  // HTTP 200 with success:false, so the provider's own explanation is readable
  // instead of being collapsed into a generic error.
  window.testEmail = function (btn) {
    var out = document.getElementById('test-email-result');
    btn.disabled = true;
    if (out) {
      out.classList.remove('d-none', 'text-success', 'text-danger');
      out.classList.add('text-muted');
      out.textContent = 'Sending…';
    }
    FerumApi.admin.testEmail().then(function (r) {
      return r.json().then(function (d) { return { ok: r.ok, body: d }; });
    }).then(function (res) {
      if (!out) return;
      out.classList.remove('text-muted');
      // A non-2xx here is the cooldown or a permission failure — a real error
      // response, not a delivery report.
      if (!res.ok) {
        out.classList.add('text-danger');
        out.textContent = Ferum.errorMessage(res.body) || 'Could not send the test email.';
        return;
      }
      var result = (res.body && res.body.data) || {};
      if (result.success) {
        out.classList.add('text-success');
        out.textContent = 'Sent to ' + result.sent_to + ' via ' + result.provider + '.';
      } else {
        out.classList.add('text-danger');
        out.textContent = 'Send failed (' + result.provider + '): ' + (result.error || 'unknown error');
      }
    }).catch(function () {
      if (!out) return;
      out.classList.remove('text-muted');
      out.classList.add('text-danger');
      out.textContent = Ferum.t('js-network-error');
    }).finally(function () {
      btn.disabled = false;
    });
  };

  window.uploadBranding = function (input, type) {
    var file = input.files[0];
    if (!file) return;
    var statusEl = document.getElementById(type + '-status');
    if (statusEl) statusEl.innerHTML = '<span class="text-muted"><i class="fa-solid fa-spinner fa-spin me-1"></i>Uploading…</span>';
    var fd = new FormData();
    fd.append('file', file);
    FerumApi.admin.uploadBranding(type, fd).then(function (r) {
      if (!r.ok) {
        return r.json().then(function (d) {
          if (statusEl) statusEl.innerHTML = '<span class="text-danger">' + Ferum.escapeHtml(Ferum.errorMessage(d) || 'Upload failed.') + '</span>';
        });
      }
      return r.json().then(function (d) {
        var url  = d.data && (d.data.logo_url || d.data.favicon_url);
        if (url) {
          var wrap = document.getElementById(type + '-preview-wrap');
          var img  = document.getElementById(type + '-preview');
          if (!img && wrap) {
            wrap.innerHTML = '<img id="' + type + '-preview" alt="" style="max-width:100%;max-height:100%;object-fit:contain">';
            img = document.getElementById(type + '-preview');
          }
          if (img) img.src = url;
          var urlField = document.getElementById('cfg-logo_url');
          if (type === 'logo' && urlField) urlField.value = url;
        }
        if (statusEl) statusEl.innerHTML = '<span class="text-success"><i class="fa-solid fa-check me-1"></i>Uploaded.</span>';
      });
    }).catch(function () {
      if (statusEl) statusEl.innerHTML = '<span class="text-danger">Network error.</span>';
    });
    input.value = '';
  };

  window.removeBranding = async function (type) {
    var ok = await Ferum.showConfirm('Remove ' + type[0].toUpperCase() + type.slice(1), 'Remove the current ' + type + '?', 'Remove');
    if (!ok) return;
    FerumApi.admin.removeBranding(type).then(function (r) {
      if (r.ok) location.reload();
      else showSettingsAlert('Failed to remove ' + type + '.', 'danger');
    }).catch(function () { showSettingsAlert(Ferum.t('js-network-error'), 'danger'); });
  };

  // ── Permissions panel (Alpine component) ─────────────────────────
  function permissionsPanelData() {
    return {
      search: '',
      activeGroup: 'all',
      permissions: JSON.parse(document.getElementById('permissions-data').textContent)
        .flatMap(function (g) {
          return (g.permissions || []).map(function (p) {
            return { key: p.key, description: p.description, group: g.name, min_trust: p.min_trust };
          });
        }),
      get groups() {
        var seen = new Set();
        var out = [];
        this.permissions.forEach(function (p) { if (!seen.has(p.group)) { seen.add(p.group); out.push(p.group); } });
        return out;
      },
      groupCount: function (g) {
        return this.permissions.filter(function (p) { return p.group === g; }).length;
      },
      get totalCount() { return this.permissions.length; },
      get filtered() {
        var s = this.search.toLowerCase().trim();
        var activeGroup = this.activeGroup;
        return this.permissions.filter(function (p) {
          var matchGroup = activeGroup === 'all' || p.group === activeGroup;
          var matchSearch = !s || p.key.toLowerCase().includes(s) || p.description.toLowerCase().includes(s);
          return matchGroup && matchSearch;
        });
      },
      trustClass: function (t) {
        return ({ leader: 'bg-warning text-dark', regular: 'bg-info text-dark', member: 'bg-success', basic: 'bg-primary' }[t]) || 'bg-secondary';
      },
      trustLabel: function (t) { return t === 'new' ? 'any' : t; },
    };
  }
  window.permissionsPanel = permissionsPanelData;

  // ── Webhooks (Alpine component) ───────────────────────────────────
  // Exposed as a plain global so Alpine can resolve it from x-data="webhooksPanel()"
  // regardless of script load order — no alpine:init timing dependency.
  window.webhooksPanel = function () {
    return {
      loading: true, webhooks: [], saving: false, webhookError: '',
      form: { url: '', secret: '', events: [] },
      editingId: null, editForm: { url: '', secret: '', events: [] }, editError: '',
      allEvents: ['post.created','post.deleted','thread.created','thread.deleted','thread.locked','thread.moved','reaction.added','reaction.removed','best_answer.marked','mention.added','user.banned','user.warned','user.followed'],
      init: function () {
        var self = this;
        var tab = document.querySelector('[data-bs-target="#tab-webhooks"]');
        if (tab) {
          tab.addEventListener('shown.bs.tab', function () { if (self.loading) self.load(); }, { once: true });
        }
      },
      load: function () {
        var self = this;
        self.loading = true;
        FerumApi.admin.getWebhooks().then(function (r) { return r.json(); })
          .then(function (d) { self.webhooks = d.data || []; })
          .catch(function () {})
          .finally(function () { self.loading = false; });
      },
      toggleWebhook: function (wh) {
        var self = this;
        wh.busy = true;
        FerumApi.admin.toggleWebhook(wh.id, !wh.is_active)
          .then(function (r) { if (r.ok) self.load(); })
          .catch(function () {})
          .finally(function () { wh.busy = false; });
      },
      deleteWebhook: async function (id) {
        var self = this;
        var ok = await Ferum.showConfirm('Delete Webhook', 'This webhook will stop receiving events. This cannot be undone.', 'Delete');
        if (!ok) return;
        var wh = self.webhooks.find(function (w) { return w.id === id; });
        if (wh) wh.busy = true;
        FerumApi.admin.deleteWebhook(id)
          .then(function (r) { if (r.ok) self.load(); })
          .catch(function () { if (wh) wh.busy = false; });
      },
      addWebhook: function () {
        var self = this;
        self.webhookError = '';
        if (!self.form.url) { self.webhookError = 'URL is required.'; return; }
        if (!/^https?:\/\//.test(self.form.url)) { self.webhookError = 'URL must start with http:// or https://'; return; }
        if (!self.form.events.length) { self.webhookError = 'Select at least one event.'; return; }
        self.saving = true;
        var body = { url: self.form.url, events: self.form.events.slice() };
        if (self.form.secret) body.secret = self.form.secret;
        FerumApi.admin.createWebhook(body).then(function (r) {
          if (r.ok) { self.form = { url: '', secret: '', events: [] }; self.load(); }
          else return r.json().then(function (d) { self.webhookError = Ferum.errorMessage(d) || 'Failed to add webhook.'; });
        }).catch(function () { self.webhookError = Ferum.t('js-network-error'); })
          .finally(function () { self.saving = false; });
      },
      testWebhook: function (wh) {
        var self = this;
        wh.testing = true;
        wh.testResult = null;
        FerumApi.admin.testWebhook(wh.id).then(function (r) { return r.json(); })
          .then(function (d) {
            var result = (d && d.data) || {};
            wh.testResult = {
              success: !!result.success,
              message: result.success
                ? 'Test delivered — HTTP ' + result.status_code
                : 'Test failed: ' + (result.error || ('HTTP ' + result.status_code)),
            };
          })
          .catch(function () { wh.testResult = { success: false, message: Ferum.t('js-network-error') }; })
          .finally(function () { wh.testing = false; self.webhooks = self.webhooks.slice(); });
      },
      startEdit: function (wh) {
        this.editingId = wh.id;
        this.editError = '';
        this.editForm = { url: wh.url, secret: '', events: wh.events.slice() };
      },
      cancelEdit: function () {
        this.editingId = null;
      },
      saveEdit: function () {
        var self = this;
        self.editError = '';
        if (!self.editForm.url) { self.editError = 'URL is required.'; return; }
        if (!/^https?:\/\//.test(self.editForm.url)) { self.editError = 'URL must start with http:// or https://'; return; }
        if (!self.editForm.events.length) { self.editError = 'Select at least one event.'; return; }
        self.saving = true;
        var body = { url: self.editForm.url, events: self.editForm.events.slice() };
        if (self.editForm.secret) body.secret = self.editForm.secret;
        FerumApi.admin.updateWebhook(self.editingId, body).then(function (r) {
          if (r.ok) { self.editingId = null; self.load(); }
          else return r.json().then(function (d) { self.editError = Ferum.errorMessage(d) || 'Failed to save changes.'; });
        }).catch(function () { self.editError = Ferum.t('js-network-error'); })
          .finally(function () { self.saving = false; });
      },
    };
  };

  // Reads the reason a plugin upload was rejected out of the response.
  //
  // The body is preferred over `res.statusText`: the server answers a bad
  // package with 400 and one plain sentence ("This package has no plugin.toml at
  // its root."), which is the only part that tells the admin what to do. This
  // used to read `res.statusText || body`, and since statusText is never empty
  // over HTTP/1.1 the body was dead code — every rejection, whatever the cause,
  // surfaced as "Bad Request" or "Internal Server Error".
  //
  // A genuine server fault still renders the themed HTML error page, so anything
  // that looks like a document is discarded rather than dumped into the alert.
  function uploadFailureMessage(res) {
    return res.text().then(
      function (body) {
        var text = (body || '').trim();
        if (!text || text.charAt(0) === '<') {
          return res.statusText || ('HTTP ' + res.status);
        }
        return text.length > 300 ? text.slice(0, 300) + '…' : text;
      },
      function () { return res.statusText || ('HTTP ' + res.status); }
    );
  }

  // ── Plugin upload modal (Alpine component) ───────────────────────
  // pluginUploadData is extracted as a named function so it can be
  // registered via both window (Alpine 2.x global lookup) and
  // Alpine.data() via alpine:init (Alpine 3.x, timing-safe).
  function pluginUploadData() {
    return {
      step: 1,
      selectedFile: null,
      loading: false,
      errorMsg: '',
      fileSelected: function (e) {
        this.selectedFile = e.target.files[0] || null;
      },
      reset: function () {
        this.step = 1;
        this.selectedFile = null;
        this.loading = false;
        this.errorMsg = '';
        var c = document.getElementById('plugin-review-container');
        if (c) c.innerHTML = '';
      },
      uploadForReview: function () {
        var self = this;
        if (!self.selectedFile) return;
        self.loading = true;
        self.errorMsg = '';
        var fd = new FormData();
        fd.append('file', self.selectedFile);
        fetch('/admin/plugins/upload-review', { method: 'POST', body: fd })
          .then(function (res) {
            if (!res.ok) {
              return uploadFailureMessage(res).then(function (msg) {
                self.errorMsg = 'Upload failed: ' + msg;
              });
            }
            return res.text().then(function (html) {
              var c = document.getElementById('plugin-review-container');
              if (c) c.innerHTML = html;
              self.step = 2;
            });
          })
          .catch(function (e) { self.errorMsg = 'Network error: ' + e.message; })
          .finally(function () { self.loading = false; });
      },
      confirmInstall: function () {
        var self = this;
        if (!self.selectedFile) return;
        self.loading = true;
        self.errorMsg = '';
        var capInput = document.querySelector('#plugin-review-container input[name="granted_capabilities"]');
        var fd = new FormData();
        fd.append('file', self.selectedFile);
        fd.append('granted_capabilities', capInput ? capInput.value : '{}');
        fetch('/admin/plugins/install', { method: 'POST', body: fd, redirect: 'follow' })
          .then(function (res) {
            if (res.redirected) { window.location.href = res.url; return; }
            if (!res.ok) {
              return uploadFailureMessage(res).then(function (msg) {
                self.errorMsg = 'Install failed: ' + msg;
              });
            }
          })
          .catch(function (e) { self.errorMsg = 'Network error: ' + e.message; })
          .finally(function () { self.loading = false; });
      },
    };
  }

  // Rebuild the granted_capabilities hidden field from the checked hook/host
  // checkboxes in the capability review step. The review partial is injected via
  // innerHTML, so it cannot carry its own <script> — this must live here instead.
  window.updatePluginGrant = function () {
    var container = document.getElementById('plugin-review-container');
    if (!container) return;

    var originalInput = container.querySelector('#plugin-capabilities-original');
    var raw = {};
    try { raw = JSON.parse(originalInput ? originalInput.value : '{}'); } catch (e) { raw = {}; }

    var hookBoxes = container.querySelectorAll('.cap-hook-checkbox');
    if (hookBoxes.length > 0) {
      var hooks = [];
      hookBoxes.forEach(function (cb) {
        if (cb.checked) {
          hooks.push({ name: cb.value, priority: parseInt(cb.getAttribute('data-priority'), 10) || 100 });
        }
      });
      raw.hooks = hooks;
    }

    var httpBoxes = container.querySelectorAll('.cap-http-checkbox');
    if (httpBoxes.length > 0) {
      var hosts = [];
      httpBoxes.forEach(function (cb) { if (cb.checked) hosts.push(cb.value); });
      raw.http_allowlist = hosts;
    }

    var rpcBoxes = container.querySelectorAll('.cap-rpc-checkbox');
    if (rpcBoxes.length > 0) {
      var rpcActions = [];
      rpcBoxes.forEach(function (cb) { if (cb.checked) rpcActions.push(cb.value); });
      raw.rpc = rpcActions;
    }

    var apiBoxes = container.querySelectorAll('.cap-api-checkbox');
    if (apiBoxes.length > 0) {
      var apiFns = [];
      apiBoxes.forEach(function (cb) { if (cb.checked) apiFns.push(cb.value); });
      raw.api = apiFns;
    }

    var dbBox = container.querySelector('#review-cap-db');
    if (dbBox) raw.db = dbBox.checked;

    var mediaBox = container.querySelector('#review-cap-media');
    if (mediaBox) raw.media = mediaBox.checked;

    var hiddenInput = container.querySelector('input[name="granted_capabilities"]');
    if (hiddenInput) hiddenInput.value = JSON.stringify(raw);
  };

  // Expose for both Alpine 2.x (global function lookup) and as a fallback.
  window.pluginUpload = pluginUploadData;

  // Register via alpine:init for Alpine 3.x — fires before Alpine walks
  // the DOM, so the component is always available regardless of script order.
  document.addEventListener('alpine:init', function () {
    if (typeof Alpine !== 'undefined' && typeof Alpine.data === 'function') {
      Alpine.data('pluginUpload', pluginUploadData);
      Alpine.data('webhooksPanel', window.webhooksPanel);
      Alpine.data('permissionsPanel', permissionsPanelData);
    }
  });

  // ── Threads page ──────────────────────────────────────────────────
  // `slug`, not id: DELETE /api/threads/{slug} resolves the thread by slug.
  window.deleteThread = async function (slug, title) {
    var ok = await Ferum.showConfirm('Delete Thread', 'Delete "' + title + '"? This will remove all posts. This cannot be undone.', 'Delete thread');
    if (!ok) return;
    FerumApi.threads.delete(slug).then(function (res) {
      if (res.ok) location.reload();
      else res.json().then(function (b) { Ferum.toast(Ferum.errorMessage(b) || 'Failed to delete thread.', true); }).catch(function () { Ferum.toast('Failed to delete thread.', true); });
    });
  };

  // ── Reports page ──────────────────────────────────────────────────
  window.resolveReport = function (id, status) {
    FerumApi.mod.resolveReport(id, status).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed.', true); }).catch(function () { Ferum.toast('Failed.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  // ── User detail page ───────────────────────────────────────────────
  window.unbanUser = async function (userId) {
    var ok = await Ferum.showConfirm('Unban User', 'Remove the ban from this user?', 'Unban');
    if (!ok) return;
    FerumApi.admin.unbanUser(userId).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed to unban.', true); }).catch(function () { Ferum.toast('Failed to unban.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  window.revokeRole = async function (userId, roleId, roleName) {
    var ok = await Ferum.showConfirm('Remove Role', 'Remove role "' + roleName + '" from this user?', 'Remove');
    if (!ok) return;
    FerumApi.admin.revokeRole(userId, roleId).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed to remove role.', true); }).catch(function () { Ferum.toast('Failed to remove role.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  window.assignRoleById = async function (userId) {
    var select = document.getElementById('role-select');
    var roleId = select && select.value;
    if (!roleId) { Ferum.toast('Please select a role first.', true); return; }
    var roleName = select.options[select.selectedIndex].text.trim();
    var isAdmin = /admin/i.test(roleName);
    if (isAdmin) {
      var ok = await Ferum.showConfirm(
        'Assign Administrator Role',
        'You are about to grant the "' + roleName + '" role, which carries full site permissions. Are you sure?',
        'Assign',
        'danger'
      );
      if (!ok) return;
    }
    FerumApi.admin.assignRole(userId, roleId).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed to assign role.', true); }).catch(function () { Ferum.toast('Failed to assign role.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  window.editUserProfile = function (userId) {
    var displayName = (document.getElementById('edit-display-name').value || '').trim() || null;
    var bio         = (document.getElementById('edit-bio').value         || '').trim() || null;
    var website     = (document.getElementById('edit-website').value     || '').trim() || null;
    if (website && !/^https?:\/\//.test(website)) {
      Ferum.showFeedback('edit-profile-feedback', 'danger', 'Website must start with http:// or https://');
      return;
    }
    FerumApi.admin.editUser(userId, { display_name: displayName, bio: bio, website: website }).then(function (r) {
      if (r.ok) {
        Ferum.showFeedback('edit-profile-feedback', 'success', 'Profile updated.');
      } else {
        r.json().then(function (d) { Ferum.showFeedback('edit-profile-feedback', 'danger', Ferum.errorMessage(d) || 'Failed to save.'); }).catch(function () { Ferum.showFeedback('edit-profile-feedback', 'danger', 'Failed to save.'); });
      }
    }).catch(function () { Ferum.showFeedback('edit-profile-feedback', 'danger', Ferum.t('js-network-error')); });
  };

  window.setUserTrustLevel = async function (userId) {
    var select = document.getElementById('trust-level-select');
    var level  = select && select.value;
    if (!level) return;
    var ok = await Ferum.showConfirm('Change Trust Level', 'Set trust level to "' + level + '"? This overrides automatic calculation.', 'Confirm');
    if (!ok) return;
    FerumApi.admin.setTrustLevel(userId, level).then(function (r) {
      if (r.ok) {
        Ferum.showFeedback('trust-level-feedback', 'success', 'Trust level updated.');
      } else {
        r.json().then(function (d) { Ferum.showFeedback('trust-level-feedback', 'danger', Ferum.errorMessage(d) || 'Failed.'); }).catch(function () { Ferum.showFeedback('trust-level-feedback', 'danger', 'Failed.'); });
      }
    }).catch(function () { Ferum.showFeedback('trust-level-feedback', 'danger', Ferum.t('js-network-error')); });
  };

  window.unlockUser = async function (userId) {
    var ok = await Ferum.showConfirm('Unlock Account', 'Clear login lock and reset failed login counter?', 'Unlock');
    if (!ok) return;
    FerumApi.admin.unlockUser(userId).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed to unlock.', true); }).catch(function () { Ferum.toast('Failed to unlock.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  window.verifyUserEmail = async function (userId) {
    var ok = await Ferum.showConfirm('Verify Email', 'Mark this user\'s email as verified?', 'Verify');
    if (!ok) return;
    FerumApi.admin.verifyUserEmail(userId).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed to verify.', true); }).catch(function () { Ferum.toast('Failed to verify.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  // ── Moderators page ───────────────────────────────────────────────
  window.revokeModerator = async function (categoryId, userId, displayName) {
    var ok = await Ferum.showConfirm('Remove Moderator', 'Remove ' + displayName + ' as moderator for this category?', 'Remove');
    if (!ok) return;
    FerumApi.admin.revokeModerator(categoryId, userId).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'Failed to remove moderator.', true); }).catch(function () { Ferum.toast('Failed to remove moderator.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  window.assignModerator = function (btn) {
    var categoryId = btn.dataset.categoryId;
    var input = btn.closest('.card-body').querySelector('input[name=username]');
    var username = input && input.value.trim();
    if (!username) { Ferum.toast('Username is required.', true); return; }
    btn.disabled = true;
    FerumApi.admin.lookupUsers(username).then(function (r) {
      return r.json();
    }).then(function (d) {
      var users = d.data || [];
      // Match on exact @username via sublabel (e.g. "@alice")
      var user = users.find(function (u) { return u.sublabel === '@' + username; });
      if (!user) {
        Ferum.toast('User "@' + username + '" not found.', true);
        btn.disabled = false;
        return;
      }
      return FerumApi.admin.assignModerator(categoryId, user.value).then(function (r2) {
        if (r2.ok) location.reload();
        else r2.json().then(function (d2) { Ferum.toast(Ferum.errorMessage(d2) || 'Failed to assign moderator.', true); btn.disabled = false; }).catch(function () { Ferum.toast('Failed to assign moderator.', true); btn.disabled = false; });
      });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); btn.disabled = false; });
  };

  window.deleteRole = async function (id, name, memberCount) {
    var count = parseInt(memberCount, 10) || 0;
    var body  = count > 0
      ? count + ' user' + (count === 1 ? '' : 's') + ' will lose this role. This cannot be undone.'
      : 'Delete role "' + name + '"? This cannot be undone.';
    var ok = await Ferum.showConfirm('Delete Role', body, 'Delete');
    if (!ok) return;
    FerumApi.admin.deleteRole(id).then(function (r) {
      if (r.ok) location.reload();
      else r.json().then(function (b) { Ferum.toast(Ferum.errorMessage(b) || 'Failed to delete role.', true); }).catch(function () { Ferum.toast('Failed to delete role.', true); });
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  // ── Roles page ────────────────────────────────────────────────────
  var SLUG_ROLE_RE = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/;

  window.createRole = function (btn) {
    var m    = document.getElementById('newRoleModal');
    var name = m && m.querySelector('[name=name]').value.trim();
    var slug = m && m.querySelector('[name=slug]').value.trim();
    if (!name) { Ferum.showFeedback('new-role-feedback', 'warning', 'Name is required.'); return; }
    if (!slug)  { Ferum.showFeedback('new-role-feedback', 'warning', 'Slug is required.'); return; }
    if (!SLUG_ROLE_RE.test(slug)) { Ferum.showFeedback('new-role-feedback', 'warning', 'Slug: lowercase letters, digits and hyphens only, no leading/trailing hyphens.'); return; }
    var body = {
      name:  name,
      slug:  slug,
      color: m.querySelector('[name=color]').value || null,
    };
    var spinner = btn.querySelector('.fa-spinner');
    btn.disabled = true;
    if (spinner) spinner.classList.remove('d-none');
    FerumApi.admin.createRole(body).then(function (r) {
      if (r.ok) location.reload();
      else {
        r.json().then(function (d) { Ferum.showFeedback('new-role-feedback', 'danger', Ferum.errorMessage(d) || 'An error occurred.'); }).catch(function () { Ferum.showFeedback('new-role-feedback', 'danger', 'An error occurred.'); });
        btn.disabled = false;
        if (spinner) spinner.classList.add('d-none');
      }
    }).catch(function () {
      Ferum.showFeedback('new-role-feedback', 'danger', Ferum.t('js-network-error'));
      btn.disabled = false;
      if (spinner) spinner.classList.add('d-none');
    });
  };

  function toRoleSlug(s) {
    return s.toLowerCase()
      .replace(/[^a-z0-9\s-]/g, '')
      .trim()
      .replace(/\s+/g, '-')
      .replace(/-{2,}/g, '-')
      .replace(/^-|-$/g, '');
  }

  (function initRolesModal() {
    var editModal = document.getElementById('editRoleModal');
    if (editModal) {
      editModal.addEventListener('show.bs.modal', function (e) {
        var btn = e.relatedTarget;
        editModal.querySelector('[name=name]').value     = (btn && btn.dataset.name)     || '';
        editModal.querySelector('[name=color]').value    = (btn && btn.dataset.color)    || '#6c757d';
        var cb = editModal.querySelector('[name=is_default]');
        if (cb) cb.checked = (btn && btn.dataset.isDefault) === 'true';
        editModal._roleId = btn && btn.dataset.id;
      });
    }

    var newModal = document.getElementById('newRoleModal');
    if (!newModal) return;
    var nameInput = newModal.querySelector('[name=name]');
    var slugInput = newModal.querySelector('[name=slug]');
    if (!nameInput || !slugInput) return;

    var slugEditedByUser = false;
    slugInput.addEventListener('input', function () {
      slugEditedByUser = slugInput.value.length > 0;
    });
    nameInput.addEventListener('input', function () {
      if (!slugEditedByUser) {
        slugInput.value = toRoleSlug(nameInput.value);
      }
    });
    newModal.addEventListener('hidden.bs.modal', function () {
      slugEditedByUser = false;
      nameInput.value = '';
      slugInput.value = '';
      newModal.querySelector('[name=color]').value = '#6c757d';
    });
  }());

  window.saveRole = function (btn) {
    var m  = document.getElementById('editRoleModal');
    var id = m && m._roleId;
    if (!id) return;
    var name = m.querySelector('[name=name]').value.trim();
    if (!name) { Ferum.toast('Role name is required.', true); return; }
    var body = {
      name:       name,
      color:      m.querySelector('[name=color]').value           || null,
      is_default: m.querySelector('[name=is_default]').checked,
    };
    var spinner = btn.querySelector('.fa-spinner');
    btn.disabled = true;
    if (spinner) spinner.classList.remove('d-none');
    FerumApi.admin.updateRole(id, body).then(function (r) {
      if (r.ok) location.reload();
      else {
        r.json().then(function (d) { Ferum.toast(Ferum.errorMessage(d) || 'An error occurred.', true); }).catch(function () { Ferum.toast('An error occurred.', true); });
        btn.disabled = false;
        if (spinner) spinner.classList.add('d-none');
      }
    }).catch(function () {
      Ferum.toast(Ferum.t('js-network-error'), true);
      btn.disabled = false;
      if (spinner) spinner.classList.add('d-none');
    });
  };

  // ── Categories page ───────────────────────────────────────────────
  (function initCategoriesModal() {
    var editModal = document.getElementById('editCategoryModal');
    if (!editModal) return;
    editModal.addEventListener('show.bs.modal', function (e) {
      var btn = e.relatedTarget;
      editModal.querySelector('[name=name]').value        = (btn && btn.dataset.name)        || '';
      editModal.querySelector('[name=slug]').value        = (btn && btn.dataset.slug)        || '';
      editModal.querySelector('[name=description]').value = (btn && btn.dataset.description) || '';
      editModal.querySelector('[name=view_policy]').value = (btn && btn.dataset.viewPolicy)  || 'public';
      editModal.querySelector('[name=post_policy]').value = (btn && btn.dataset.postPolicy)  || 'members';
      editModal.querySelector('[name=color]').value       = (btn && btn.dataset.color)       || '#6c757d';
      editModal.querySelector('[name=parent_id]').value   = (btn && btn.dataset.parentId)    || '';
      editModal.querySelector('[name=position]').value    = (btn && btn.dataset.position)    || '0';
      editModal._categoryId = btn && btn.dataset.id;
    });
  }());

  function catMsgFromError(d) {
    var code    = (d && d.error && d.error.code)    || '';
    var message = Ferum.errorMessage(d) || '';
    if (code === 'slug_taken')                 return 'A category with this slug already exists. Choose a different slug.';
    if (code === 'category_has_subcategories') return 'Cannot delete: this category has subcategories. Delete or move them first.';
    if (code === 'category_has_threads')       return 'Cannot delete: this category still has threads. Move or delete the threads first.';
    if (code === 'not_found')                  return 'Category not found — it may have been deleted by another admin.';
    if (code === 'unauthorized')               return 'Your session has expired. Please reload the page and log in again.';
    if (message === 'slug_reserved')           return 'This slug is reserved and cannot be used. Choose a different slug.';
    if (message.indexOf('nesting') !== -1 || message.indexOf('2 levels') !== -1)
                                               return 'Categories can only be nested one level deep. Select a top-level category as parent.';
    return message || 'An error occurred.';
  }

  function catModalError(feedbackId, r) {
    r.clone().json()
      .then(function (d) { Ferum.showFeedback(feedbackId, 'danger', catMsgFromError(d)); })
      .catch(function () { Ferum.showFeedback(feedbackId, 'danger', 'An error occurred (HTTP ' + r.status + ').'); });
  }

  function catToastError(r) {
    r.clone().json()
      .then(function (d) { Ferum.toast(catMsgFromError(d), true); })
      .catch(function () { Ferum.toast('An error occurred.', true); });
  }

  var SLUG_RE = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/;

  window.createCategory = function (btn) {
    var m    = document.getElementById('newCategoryModal');
    var name = m && m.querySelector('[name=new_name]').value.trim();
    var slug = m && m.querySelector('[name=new_slug]').value.trim();
    if (!name || !slug) { Ferum.showFeedback('new-cat-feedback', 'warning', 'Name and slug are required.'); return; }
    if (!SLUG_RE.test(slug)) { Ferum.showFeedback('new-cat-feedback', 'warning', 'Slug: lowercase letters, digits and hyphens only, no leading/trailing hyphens.'); return; }
    var body = {
      name: name, slug: slug,
      description: m.querySelector('[name=new_description]').value.trim()  || null,
      parent_id:   m.querySelector('[name=new_parent_id]').value           || null,
      view_policy: m.querySelector('[name=new_view_policy]').value,
      post_policy: m.querySelector('[name=new_post_policy]').value,
      color:       m.querySelector('[name=new_color]').value               || null,
      position:    parseInt(m.querySelector('[name=new_position]').value, 10) || 0,
    };
    var spinner = btn.querySelector('.fa-spinner');
    btn.disabled = true;
    if (spinner) spinner.classList.remove('d-none');
    FerumApi.admin.createCategory(body).then(function (r) {
      if (r.ok) location.reload();
      else {
        catModalError('new-cat-feedback', r);
        btn.disabled = false;
        if (spinner) spinner.classList.add('d-none');
      }
    }).catch(function () {
      Ferum.showFeedback('new-cat-feedback', 'danger', 'Network error — please try again.');
      btn.disabled = false;
      if (spinner) spinner.classList.add('d-none');
    });
  };

  window.saveCategory = function (btn) {
    var m  = document.getElementById('editCategoryModal');
    var id = m && m._categoryId;
    if (!id) return;
    var slug = m.querySelector('[name=slug]').value.trim() || null;
    if (slug && !SLUG_RE.test(slug)) { Ferum.showFeedback('edit-cat-feedback', 'warning', 'Slug: lowercase letters, digits and hyphens only, no leading/trailing hyphens.'); return; }
    var body = {
      name:        m.querySelector('[name=name]').value.trim()        || null,
      slug:        slug,
      description: m.querySelector('[name=description]').value.trim() || null,
      parent_id:   m.querySelector('[name=parent_id]').value          || null,
      view_policy: m.querySelector('[name=view_policy]').value        || null,
      post_policy: m.querySelector('[name=post_policy]').value        || null,
      color:       m.querySelector('[name=color]').value              || null,
      position:    parseInt(m.querySelector('[name=position]').value, 10),
    };
    var spinner = btn.querySelector('.fa-spinner');
    btn.disabled = true;
    if (spinner) spinner.classList.remove('d-none');
    FerumApi.admin.updateCategory(id, body).then(function (r) {
      if (r.ok) location.reload();
      else {
        catModalError('edit-cat-feedback', r);
        btn.disabled = false;
        if (spinner) spinner.classList.add('d-none');
      }
    }).catch(function () {
      Ferum.showFeedback('edit-cat-feedback', 'danger', 'Network error — please try again.');
      btn.disabled = false;
      if (spinner) spinner.classList.add('d-none');
    });
  };

  window.deleteCategory = async function (id, name) {
    var label = name ? '“' + name + '”' : 'this category';
    var ok = await Ferum.showConfirm('Delete Category', 'Delete ' + label + '? This cannot be undone.', 'Delete');
    if (!ok) return;
    FerumApi.admin.deleteCategory(id).then(function (r) {
      if (r.ok) location.reload();
      else catToastError(r);
    }).catch(function () { Ferum.toast(Ferum.t('js-network-error'), true); });
  };

  // ── Ban form on users/detail page ─────────────────────────────────
  // Intercepts the Confirm Ban button, shows Ferum.showConfirm, then calls
  // the JSON API directly. Type-to-confirm is required for permanent bans.
  (function initBanForm() {
    var banForm = document.getElementById('ban-form');
    if (!banForm) return;
    document.addEventListener('click', async function (e) {
      var btn = e.target.closest('[data-admin-action="confirm-ban"]');
      if (!btn) return;
      var reason   = banForm.querySelector('[name=reason]').value.trim();
      var until    = banForm.querySelector('[name=banned_until]').value;
      var userId   = banForm.dataset.userId;
      var username = banForm.dataset.username;
      if (!reason) { Ferum.showFeedback('ban-feedback', 'warning', 'Reason is required.'); return; }
      if (until && new Date(until) <= new Date()) {
        Ferum.showFeedback('ban-feedback', 'warning', 'Ban end date must be in the future.'); return;
      }
      var permanent = !until;
      var confirmBody = permanent
        ? 'Permanently ban ' + username + '? They will not be able to log in.'
        : 'Ban ' + username + ' until ' + new Date(until).toLocaleDateString() + '.';
      var ok = await Ferum.showConfirm(
        permanent ? 'Permanent Ban' : 'Temporary Ban',
        confirmBody,
        'Ban User',
        'danger',
        permanent ? { typeToConfirm: username } : null
      );
      if (!ok) return;
      btn.disabled = true;
      // The raw <input type="datetime-local"> value is zoneless and the API
      // rejects it outright — this used to send `until` unconverted, so every
      // temporary ban from this form failed with a 422 while permanent bans
      // (which send null) worked.
      FerumApi.admin.banUser(userId, reason, Ferum.localInputToIso(until))
        .then(function (r) {
          if (r.ok) location.reload();
          else r.json().then(function (b) { Ferum.showFeedback('ban-feedback', 'danger', Ferum.errorMessage(b) || 'Failed to ban user.'); btn.disabled = false; });
        })
        .catch(function () { Ferum.showFeedback('ban-feedback', 'danger', Ferum.t('js-network-error')); btn.disabled = false; });
    });
  }());

  // ── Branding file-input change events ────────────────────────────
  document.addEventListener('change', function (e) {
    var input = e.target.closest('[data-branding-upload]');
    if (!input) return;
    window.uploadBranding(input, input.dataset.brandingUpload);
  });

  // ── Plugin capability review checkboxes ──────────────────────────
  // Delegated rather than inline `onchange=`, which the CSP blocks: `script-src`
  // grants 'self' and 'unsafe-eval' (for Alpine's expression compiler) but never
  // 'unsafe-inline', so an `on*=` attribute never fires. The review partial
  // carried six of them, which meant unticking a hook, host, RPC action or the
  // `db`/`media` grant changed nothing — `granted_capabilities` was still
  // submitted as the manifest's full request. Delegation also survives the
  // partial being replaced via innerHTML, which is how it arrives.
  document.addEventListener('change', function (e) {
    if (!e.target.closest('#plugin-review-container')) return;
    if (e.target.type !== 'checkbox') return;
    window.updatePluginGrant();
  });

  // ── Event delegation for onclick-replaced buttons ─────────────────
  document.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-admin-action]');
    if (!btn) return;
    switch (btn.dataset.adminAction) {
      case 'save-settings':    window.saveSettings(btn);                break;
      case 'test-email':       window.testEmail(btn);                   break;
      case 'remove-branding':  window.removeBranding(btn.dataset.brandingType); break;
      case 'create-category':  window.createCategory(btn);              break;
      case 'save-category':    window.saveCategory(btn);                break;
      case 'delete-category':  window.deleteCategory(btn.dataset.categoryId, btn.dataset.name); break;
      case 'resolve-report':   window.resolveReport(btn.dataset.reportId, 'resolved');  break;
      case 'dismiss-report':   window.resolveReport(btn.dataset.reportId, 'dismissed'); break;
      case 'unlock-user':      window.unlockUser(btn.dataset.userId);                                            break;
      case 'edit-profile':     window.editUserProfile(btn.dataset.userId);                                     break;
      case 'set-trust-level':  window.setUserTrustLevel(btn.dataset.userId);                                   break;
      case 'verify-email':     window.verifyUserEmail(btn.dataset.userId);                                     break;
      case 'assign-role-by-id': window.assignRoleById(btn.dataset.userId);                                    break;
      case 'unban-user':       window.unbanUser(btn.dataset.userId);                                           break;
      case 'revoke-role':      window.revokeRole(btn.dataset.userId, btn.dataset.roleId, btn.dataset.roleName); break;
      case 'assign-moderator': window.assignModerator(btn);                             break;
      case 'revoke-moderator': window.revokeModerator(btn.dataset.categoryId, btn.dataset.userId, btn.dataset.displayName); break;
      case 'create-role':      window.createRole(btn);                  break;
      case 'save-role':        window.saveRole(btn);                    break;
      case 'delete-role':      window.deleteRole(btn.dataset.roleId, btn.dataset.roleName, btn.dataset.memberCount); break;
      case 'delete-thread':    window.deleteThread(btn.dataset.threadSlug, btn.dataset.threadTitle); break;
    }
  });
}());
