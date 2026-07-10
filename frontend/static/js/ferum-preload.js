// Runs synchronously before CSS renders to prevent flash of unstyled content.
// Must be loaded as a blocking <script> (no defer/async) in <head> before stylesheets.
//
// For a logged-in user, base.html already rendered data-bs-theme/data-font-size/
// data-layout server-side from their saved (DB) preferences — marked by
// data-prefs-ssr="1" on <html>. Applying localStorage on top of that would
// silently reintroduce the exact bug this was meant to fix: a stale value
// saved on a different device/browser overriding the correct synced one.
// localStorage here is only a fallback for guests, who have no account to
// read preferences from.
(function () {
  if (document.documentElement.hasAttribute('data-prefs-ssr')) return;
  try {
    var t = localStorage.getItem('ferum-theme');
    var f = localStorage.getItem('ferum-font-size');
    var l = localStorage.getItem('ferum-layout');
    if (t) document.documentElement.setAttribute('data-bs-theme', t);
    if (f && f !== 'medium') document.documentElement.setAttribute('data-font-size', f);
    if (l && l !== 'comfortable') document.documentElement.setAttribute('data-layout', l);
  } catch (e) {}
}());
