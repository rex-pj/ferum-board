/**
 * ferum-feed — swap the thread feed in place instead of reloading the page.
 *
 * WHY THIS EXISTS
 * The sort tabs and filter chips are real links, deliberately: they work with
 * scripting off, the Back button is correct, and a chip toggles itself simply
 * by pointing at the URL without its own filter. The cost is that every click
 * is a document navigation — a white flash, a re-parse of the whole shell, and
 * a scroll reset. The `#feed` fragment on those hrefs fixes the scroll reset
 * for the no-JS path; this file removes the navigation entirely for everyone
 * else, so the scroll position is not restored — it is never disturbed.
 *
 * NO BACKEND INVOLVED
 * It fetches the very same URL the link points at and lifts the feed out of the
 * response with DOMParser. There is no partial-render endpoint to keep in sync
 * with the page, which is the usual way this kind of enhancement rots: the
 * fragment and the full page drift, and only one of them gets fixed.
 *
 * IT DEGRADES BY GETTING OUT OF THE WAY
 * Every failure path — no feed on the page, an unexpected response shape, a
 * non-OK status, a network error — falls back to letting the browser navigate.
 * A silently stale list is worse than the reload this was avoiding.
 */
(function () {
  'use strict';

  var CONTROL_SEL = '.fr-sort-tab, .fr-filter-chip';
  var PAGER_SEL   = '.pagination .page-link';
  var LOADING_CLS = 'fr-feed--loading';

  var inflight = null;

  // Anchored on the CONTROLS, not on `.fr-feed`. Seven templates render a
  // `.fr-feed` (bookmarks, search, profiles, the forum index…) and only two
  // render these controls, so keying off the feed would arm this file on pages
  // whose pagination means something entirely different.
  function feedIn(root) {
    var controls = root.querySelector('.fr-feed-controls');
    return controls ? controls.closest('.fr-feed') : null;
  }

  /** The pagination <nav> belonging to a feed — a SIBLING of it, not a child. */
  function pagerFor(feed) {
    if (!feed || !feed.parentElement) return null;
    var list = feed.parentElement.querySelector('.pagination');
    return list ? list.closest('nav') : null;
  }

  function setBusy(feed, busy) {
    if (!feed) return;
    feed.classList.toggle(LOADING_CLS, busy);
    if (busy) feed.setAttribute('aria-busy', 'true');
    else feed.removeAttribute('aria-busy');
  }

  // ── Focus ───────────────────────────────────────────────────────────
  // The swap deletes the element the user is standing on, which drops focus to
  // <body> — a keyboard user would have to Tab back from the top of the page
  // after every click. Only worth restoring when focus was actually ON the
  // link: a mouse or touch click frequently leaves it elsewhere, and forcing it
  // there would paint a focus ring nobody asked for.
  function focusToken(link) {
    if (document.activeElement !== link) return null;
    return link.getAttribute('data-feed-key') || 'feed';
  }

  function restoreFocus(token) {
    if (!token) return;
    var feed = feedIn(document);
    if (!feed) return;

    var el = token === 'feed'
      ? null
      : feed.querySelector('[data-feed-key="' + token + '"]');

    // A pager link, or a control that no longer exists: fall back to the feed
    // itself so the reading order resumes at the list rather than the page top.
    if (!el) { el = feed; el.setAttribute('tabindex', '-1'); }

    // preventScroll is not optional here — focus() scrolls the target into view
    // by default, which would undo the exact thing this whole file exists for.
    el.focus({ preventScroll: true });
  }

  function withTransition(apply) {
    var reduce = window.matchMedia &&
                 window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!document.startViewTransition || reduce) { apply(); return; }
    document.startViewTransition(apply);
  }

  /** Move the fetched feed (and its pager) into the live document. */
  function applyDocument(doc) {
    var oldFeed = feedIn(document);
    var newFeed = feedIn(doc);
    if (!oldFeed || !newFeed) return false;

    var oldPager = pagerFor(oldFeed);
    var newPager = pagerFor(newFeed);

    withTransition(function () {
      var feed = document.importNode(newFeed, true);
      oldFeed.replaceWith(feed);

      // Four cases, because changing the sort resets to page 1: the pager can
      // survive, appear, or vanish.
      if (oldPager && newPager)      oldPager.replaceWith(document.importNode(newPager, true));
      else if (oldPager)             oldPager.remove();
      else if (newPager)             feed.after(document.importNode(newPager, true));

      // Server-rendered timestamps are UTC and are localised by a pass that ran
      // at load. Nodes injected afterwards have to ask for it, or every row in
      // the swapped-in list silently reverts to UTC.
      if (window.Ferum && window.Ferum.initTimestamps) window.Ferum.initTimestamps();
    });

    var title = doc.querySelector('title');
    if (title) document.title = title.textContent;
    return true;
  }

  function load(href, push, focusKey) {
    var feed = feedIn(document);
    if (!feed) return;

    // A reader tapping through sorts outruns the network. Without this the
    // responses race and the list can settle on the older one.
    if (inflight) inflight.abort();
    var ctl = window.AbortController ? new AbortController() : null;
    inflight = ctl;
    setBusy(feed, true);

    // `fetch` is monkey-patched by ferum-progress.js, so the top-of-page bar
    // covers a slow swap for free — but only past its 120ms show-delay, which
    // is the behaviour we want here too.
    fetch(href, {
      credentials: 'same-origin',
      headers: { 'Accept': 'text/html' },
      signal: ctl ? ctl.signal : undefined
    })
      .then(function (res) {
        if (!res.ok) throw new Error('HTTP ' + res.status);
        return res.text();
      })
      .then(function (html) {
        var doc = new DOMParser().parseFromString(html, 'text/html');
        if (!applyDocument(doc)) throw new Error('feed not found in response');
        restoreFocus(focusKey);
        // Keeps the `#feed` the link carried, so a later F5 still lands on the
        // controls rather than the top of the page.
        if (push) history.pushState({ ferumFeed: true }, '', href);
      })
      .catch(function (err) {
        if (err && err.name === 'AbortError') return;
        window.location.href = href;
      })
      .finally(function () {
        if (inflight !== ctl) return;   // a newer request owns the state now
        inflight = null;
        setBusy(feedIn(document), false);
      });
  }

  function interceptable(e, link) {
    if (e.defaultPrevented || e.button !== 0) return false;
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return false;
    if (!link || !link.href) return false;
    if (link.target && link.target !== '_self') return false;
    return new URL(link.href, window.location.href).origin === window.location.origin;
  }

  document.addEventListener('click', function (e) {
    if (!e.target || !e.target.closest) return;

    var link = e.target.closest(CONTROL_SEL + ', ' + PAGER_SEL);
    if (!interceptable(e, link)) return;

    var feed = feedIn(document);
    if (!feed) return;

    // A page link only belongs to us when it is in THIS feed's pager. The
    // pagination partial is shared with the thread view, search, profiles and
    // more; those pages have no feed controls and reach the guard above, but
    // this keeps the rule local rather than resting on that.
    if (!link.matches(CONTROL_SEL) && link.closest('nav') !== pagerFor(feed)) return;

    e.preventDefault();
    load(new URL(link.href, window.location.href).href, true, focusToken(link));
  });

  // Back/forward. Only same-document entries fire popstate, and inside this
  // document those are exactly the ones pushed above plus the original load —
  // all of which this can rebuild by refetching the current URL.
  window.addEventListener('popstate', function () {
    if (feedIn(document)) load(window.location.href, false);
  });
}());
