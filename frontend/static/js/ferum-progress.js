/**
 * ferum-progress — thin top-of-page progress bar for all fetch() calls.
 *
 * Monkey-patches window.fetch so every in-flight request advances the bar
 * automatically. A 120 ms show-delay suppresses the bar for fast background
 * requests (notification polling, watch/mute toggles) so it only appears for
 * operations the user is visibly waiting for.
 *
 * Color picks up --ferum-primary from the active theme; falls back to indigo.
 */
(function () {
  'use strict';

  var bar = null;
  var count = 0;
  var showTimer = null;
  var hideTimer = null;

  // ── Lazy DOM insertion ─────────────────────────────────────────────
  // Called on the first fetch that crosses the show-delay threshold.
  function getBar() {
    if (bar) return bar;

    var style = document.createElement('style');
    style.textContent =
      '#fr-progress{' +
        'position:fixed;top:0;left:0;height:3px;width:0;' +
        'z-index:9999;pointer-events:none;' +
        'background:var(--ferum-primary,#6366f1);' +
        'opacity:0;' +
      '}';
    document.head.appendChild(style);

    bar = document.createElement('div');
    bar.id = 'fr-progress';
    document.body.appendChild(bar);
    return bar;
  }

  // ── Core state machine ─────────────────────────────────────────────

  function start() {
    count += 1;
    if (count !== 1) return;

    clearTimeout(hideTimer);

    // Wait 120 ms before showing — fast requests (< 120 ms) never trigger the bar.
    showTimer = setTimeout(function () {
      var b = getBar();
      // Reset without transition, then start the slow crawl toward 80%.
      b.style.transition = 'none';
      b.style.opacity    = '1';
      b.style.width      = '0';
      void b.offsetWidth; // force reflow so the reset takes effect
      b.style.transition = 'width 8s cubic-bezier(.05,.05,0,1)';
      b.style.width      = '80%';
    }, 120);
  }

  function done() {
    count = Math.max(0, count - 1);
    if (count > 0) return;

    clearTimeout(showTimer);

    // If the bar was never shown (fast request), nothing more to do.
    if (!bar || bar.style.opacity === '0' || bar.style.opacity === '') return;

    // Snap to 100%, then fade out.
    bar.style.transition = 'width .15s ease';
    bar.style.width      = '100%';

    hideTimer = setTimeout(function () {
      bar.style.transition = 'opacity .25s ease';
      bar.style.opacity    = '0';
      // Reset width after the fade so the next start begins clean.
      setTimeout(function () { bar.style.width = '0'; }, 280);
    }, 120);
  }

  // ── Monkey-patch window.fetch ──────────────────────────────────────
  var _fetch = window.fetch;
  window.fetch = function () {
    start();
    return _fetch.apply(this, arguments).then(
      function (res) { done(); return res; },
      function (err) { done(); throw err; }
    );
  };

}());
