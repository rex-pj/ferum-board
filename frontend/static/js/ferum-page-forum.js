(function () {
  'use strict';

  // Alpine.js factory for the category watch/mute toggle panel.
  window.categoryWatchState = function (categoryId) {
    return {
      categoryId: categoryId,
      watched: false,
      muted: false,
      loading: false,
      async init() {
        try {
          var res = await FerumApi.users.getPreferences();
          if (res.ok) {
            var data  = await res.json();
            var prefs = data.data || data;
            this.watched = (prefs.watched_categories || []).includes(this.categoryId);
            this.muted   = (prefs.muted_categories   || []).includes(this.categoryId);
          }
        } catch (_) {}
      },
      async _updatePrefs(update) {
        this.loading = true;
        try {
          var res = await FerumApi.users.getPreferences();
          if (!res.ok) return;
          var data    = await res.json();
          var prefs   = data.data || data;
          var updated = update(prefs);
          await FerumApi.users.updatePreferences(updated);
        } catch (_) {}
        this.loading = false;
      },
      async toggleWatch() {
        var catId     = this.categoryId;
        var willWatch = !this.watched;
        await this._updatePrefs(function (prefs) {
          return Object.assign({}, prefs, {
            watched_categories: willWatch
              ? [...(prefs.watched_categories || []).filter(function (id) { return id !== catId; }), catId]
              : (prefs.watched_categories || []).filter(function (id) { return id !== catId; }),
            muted_categories: willWatch
              ? (prefs.muted_categories || []).filter(function (id) { return id !== catId; })
              : (prefs.muted_categories || []),
          });
        });
        this.watched = willWatch;
        if (willWatch) this.muted = false;
      },
      async toggleMute() {
        var catId    = this.categoryId;
        var willMute = !this.muted;
        await this._updatePrefs(function (prefs) {
          return Object.assign({}, prefs, {
            muted_categories: willMute
              ? [...(prefs.muted_categories || []).filter(function (id) { return id !== catId; }), catId]
              : (prefs.muted_categories || []).filter(function (id) { return id !== catId; }),
            watched_categories: willMute
              ? (prefs.watched_categories || []).filter(function (id) { return id !== catId; })
              : (prefs.watched_categories || []),
          });
        });
        this.muted = willMute;
        if (willMute) this.watched = false;
      },
    };
  };
}());
