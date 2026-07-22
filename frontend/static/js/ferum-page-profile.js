(function () {
  'use strict';

  // Read username from hidden data element
  var pd = document.getElementById('ferum-profile-data');
  var profileUsername = (pd && pd.dataset.username) || '';
  var profileId       = (pd && pd.dataset.profileId) || '';

  window.followState = function (userId) {
    return {
      userId: userId,
      following: false,
      loading: false,
      async init() {
        try {
          var res = await FerumApi.users.getFollowStatus(userId);
          if (res.ok) {
            var data = await res.json();
            this.following = (data.data && data.data.following) || false;
          }
        } catch (_) {}
      },
      async toggle() {
        this.loading = true;
        try {
          var res = this.following
            ? await FerumApi.users.unfollow(this.userId)
            : await FerumApi.users.follow(this.userId);
          if (res.ok) this.following = !this.following;
        } catch (_) {}
        this.loading = false;
      },
    };
  };

  window.profileTabs = function (pid) {
    var username = profileUsername;
    var loaded = { posts: false, followers: false, following: false };

    function buildEmptyState(iconClass, msg) {
      var wrap  = document.createElement('div');
      wrap.className = 'fr-feed';
      var inner = document.createElement('div');
      inner.className = 'text-center py-5 text-secondary';
      var icon  = document.createElement('i');
      icon.className = iconClass + ' fa-3x mb-3 d-block opacity-50';
      icon.setAttribute('aria-hidden', 'true');
      inner.appendChild(icon);
      inner.appendChild(document.createTextNode(msg));
      wrap.appendChild(inner);
      return wrap;
    }

    function buildAvatar(avatarUrl, name, extraClass, size) {
      var initial = (name || '?')[0].toUpperCase();
      if (avatarUrl) {
        var img = document.createElement('img');
        img.src = avatarUrl; img.className = extraClass;
        img.width = size; img.height = size; img.alt = ''; img.loading = 'lazy';
        return img;
      }
      var div = document.createElement('div');
      div.className = 'rounded-circle bg-secondary d-flex align-items-center justify-content-center text-white fw-bold flex-shrink-0';
      div.style.cssText = 'width:' + size + 'px;height:' + size + 'px';
      div.textContent = initial;
      return div;
    }

    function renderUserList(users, containerId) {
      var el = document.getElementById(containerId);
      if (!el) return;
      el.innerHTML = '';
      if (!users.length) { el.appendChild(buildEmptyState('fa-solid fa-users', Ferum.t('js-nobody-here-yet'))); return; }
      var feed = document.createElement('div'); feed.className = 'fr-feed';
      var list = document.createElement('div'); list.className = 'list-group list-group-flush';
      users.forEach(function (item) {
        var u = item.user || {};
        var displayName = u.display_name || u.username || '';
        var a = document.createElement('a');
        a.href = '/u/' + encodeURIComponent(u.username || '');
        a.className = 'list-group-item list-group-item-action d-flex align-items-center py-2 text-decoration-none';
        var av = buildAvatar(u.avatar_url, displayName, 'rounded-circle me-2 flex-shrink-0', 36);
        a.appendChild(av);
        var info    = document.createElement('div');
        var nameEl  = document.createElement('div'); nameEl.className  = 'fw-semibold small'; nameEl.textContent  = displayName;
        var handle  = document.createElement('div'); handle.className  = 'text-muted';         handle.style.fontSize = '.75rem'; handle.textContent = '@' + (u.username || '');
        info.appendChild(nameEl); info.appendChild(handle); a.appendChild(info); list.appendChild(a);
      });
      feed.appendChild(list); el.appendChild(feed);
    }

    function renderPostList(posts, containerId) {
      var el = document.getElementById(containerId);
      if (!el) return;
      el.innerHTML = '';
      if (!posts.length) { el.appendChild(buildEmptyState('fa-regular fa-comment', Ferum.t('js-no-posts-yet'))); return; }
      var feed = document.createElement('div'); feed.className = 'fr-feed';
      var col  = document.createElement('div'); col.className  = 'd-flex flex-column';
      posts.forEach(function (p) {
        var au     = p.author || {};
        var auName = au.display_name || au.username || '?';
        var snippet = (p.content_md || '').length > 200 ? (p.content_md || '').slice(0, 200) + '…' : (p.content_md || '');
        var card  = document.createElement('div');  card.className  = 'card border-0 border-bottom rounded-0';
        var body  = document.createElement('div');  body.className  = 'card-body py-2 px-3';
        var row   = document.createElement('div');  row.className   = 'd-flex align-items-start gap-2';
        var av    = buildAvatar(au.avatar_url, auName, 'fr-avatar fr-avatar--32 flex-shrink-0', 32);
        row.appendChild(av);
        var content = document.createElement('div'); content.className = 'flex-grow-1 min-w-0';
        var meta    = document.createElement('div'); meta.className    = 'd-flex justify-content-between align-items-center mb-1 flex-wrap gap-1';
        if (p.thread_slug) {
          var link = document.createElement('a');
          link.href = '/forum/t/' + encodeURIComponent(p.thread_slug);
          link.className = 'text-decoration-none fw-semibold small text-truncate';
          link.textContent = p.thread_title || Ferum.t('js-view-thread');
          meta.appendChild(link);
        }
        var dateEl = document.createElement('span'); dateEl.className = 'text-muted ms-auto flex-shrink-0'; dateEl.style.fontSize = '.75rem'; dateEl.textContent = new Date(p.created_at).toLocaleDateString();
        meta.appendChild(dateEl);
        var text = document.createElement('p'); text.className = 'mb-0 small text-secondary'; text.style.whiteSpace = 'pre-line'; text.textContent = snippet;
        content.appendChild(meta); content.appendChild(text); row.appendChild(content); body.appendChild(row); card.appendChild(body); col.appendChild(card);
      });
      feed.appendChild(col); el.appendChild(feed);
    }

    function showTabSkeleton(containerId) {
      var el = document.getElementById(containerId);
      if (el) el.innerHTML = '<div class="py-5 text-center text-muted"><i class="fa-solid fa-spinner fa-spin fa-lg" aria-hidden="true"></i></div>';
    }

    async function loadTab(tab) {
      if (loaded[tab]) return;
      loaded[tab] = true;
      showTabSkeleton(tab + '-content');
      var url = tab === 'posts'
        ? '/api/users/' + username + '/posts?per_page=20'
        : '/api/users/' + (pid || profileId) + '/' + tab + '?per_page=50';
      try {
        var res = await FerumApi.users.fetchUrl(url);
        if (!res.ok) throw new Error();
        var json = await res.json();
        if (tab === 'posts') renderPostList(json.data || [], 'posts-content');
        else renderUserList(json.data || [], tab + '-content');
      } catch (_) {
        var errEl = document.getElementById(tab + '-content');
        if (errEl) {
          var msg = document.createElement('div');
          msg.className = 'text-center py-4 text-danger small';
          msg.textContent = Ferum.t('js-failed-to-load');
          errEl.innerHTML = ''; errEl.appendChild(msg);
        }
      }
    }

    return {
      active: 'threads',
      setTab: function (tab) {
        this.active = tab;
        if (tab !== 'threads') loadTab(tab);
      },
    };
  };
}());
