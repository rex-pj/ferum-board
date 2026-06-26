// Runs synchronously before CSS renders to prevent flash of unstyled content.
// Must be loaded as a blocking <script> (no defer/async) in <head> before stylesheets.
(function () {
  try {
    var t = localStorage.getItem('ferum-theme');
    var f = localStorage.getItem('ferum-font-size');
    var l = localStorage.getItem('ferum-layout');
    if (t) document.documentElement.setAttribute('data-bs-theme', t);
    if (f && f !== 'medium') document.documentElement.setAttribute('data-font-size', f);
    if (l && l !== 'comfortable') document.documentElement.setAttribute('data-layout', l);
  } catch (e) {}
}());
