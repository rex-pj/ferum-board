// Runs synchronously before CSS renders to prevent flash of unstyled content.
// Must be loaded as a blocking <script> (no defer/async) in <head> before stylesheets.
//
// ── This file is the ONLY place 'auto' is interpreted ───────────────────────
// The stylesheets define exactly two token blocks, `[data-bs-theme="light"]`
// (aliased to bare `:root`) and `[data-bs-theme="dark"]`. There is no
// `@media (prefers-color-scheme: dark)` rule anywhere in the CSS, so
// `data-bs-theme="auto"` — and equally the ABSENCE of the attribute — renders
// LIGHT on every machine regardless of the OS setting.
//
// So `auto` has to be resolved to a concrete value in JS before first paint,
// and it has to be resolved in one place. It previously was not: the CSS
// resolved it to light while the toggle in ferum-utils.js resolved it from
// matchMedia, and on an OS set to dark those two disagreed — the first click
// computed "you are currently dark, go light", wrote `light`, and changed
// nothing on screen, so dark mode took two clicks. Anything that needs the
// effective theme calls FerumTheme below rather than re-deriving it.
//
// The PREFERENCE and the RESOLVED VALUE are stored separately, because
// collapsing them loses information: `data-theme-pref` keeps the user's actual
// choice ('auto' included) so that auto keeps following the OS afterwards,
// while `data-bs-theme` always carries a concrete light/dark for the CSS.
//
// For a logged-in user, base.html already rendered data-bs-theme/data-font-size/
// data-layout server-side from their saved (DB) preferences — marked by
// data-prefs-ssr="1" on <html>. Applying localStorage on top of that would
// silently reintroduce the exact bug this was meant to fix: a stale value
// saved on a different device/browser overriding the correct synced one.
// localStorage here is only a fallback for guests, who have no account to
// read preferences from. Note that the SSR value is still the *preference*, so
// it goes through the same resolution below — skipping that is what used to
// leave a logged-in "auto" user stuck in light mode.
(function () {
  var el = document.documentElement;

  function prefersDark() {
    try { return window.matchMedia('(prefers-color-scheme: dark)').matches; }
    catch (e) { return false; }
  }

  // 'auto' (or anything unrecognised) follows the OS; the two explicit values
  // are honoured as given.
  function resolve(pref) {
    if (pref === 'dark' || pref === 'light') return pref;
    return prefersDark() ? 'dark' : 'light';
  }

  function apply(pref) {
    var p = (pref === 'dark' || pref === 'light') ? pref : 'auto';
    el.setAttribute('data-theme-pref', p);
    el.setAttribute('data-bs-theme', resolve(p));
  }

  // Starting point is whatever the server rendered: the user's DB preference
  // when signed in, otherwise the active theme's own color_scheme. A guest's
  // localStorage choice overrides it; a signed-in user's does not (see above).
  var pref = el.getAttribute('data-bs-theme') || 'auto';

  if (!el.hasAttribute('data-prefs-ssr')) {
    try {
      var t = localStorage.getItem('ferum-theme');
      if (t) pref = t;
      var f = localStorage.getItem('ferum-font-size');
      var l = localStorage.getItem('ferum-layout');
      if (f && f !== 'medium') el.setAttribute('data-font-size', f);
      if (l && l !== 'comfortable') el.setAttribute('data-layout', l);
    } catch (e) {}
  }

  apply(pref);

  // Without this, 'auto' would only mean "the OS setting as it was at page
  // load" — it must keep tracking a change made while the page is open.
  try {
    var mq = window.matchMedia('(prefers-color-scheme: dark)');
    var onChange = function () {
      if (el.getAttribute('data-theme-pref') === 'auto') apply('auto');
    };
    if (mq.addEventListener) mq.addEventListener('change', onChange);
    else if (mq.addListener) mq.addListener(onChange);   // Safari < 14
  } catch (e) {}

  window.FerumTheme = {
    resolve: resolve,
    apply: apply,
    // The user's stored intent — 'auto' | 'light' | 'dark'.
    pref: function () { return el.getAttribute('data-theme-pref') || 'auto'; },
    // What is actually on screen right now — 'light' | 'dark', never 'auto'.
    current: function () {
      return el.getAttribute('data-bs-theme') === 'dark' ? 'dark' : 'light';
    }
  };
}());
