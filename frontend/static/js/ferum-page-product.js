// Product detail page — thumbnail gallery.
// Clicking (or arrow-keying) a thumbnail swaps the main image. No inline
// handlers (CSP forbids on* attributes); all wiring is delegated here.
(function () {
  'use strict';

  var main = document.getElementById('pdp-main-image');
  var strip = document.getElementById('pdp-thumbs');
  if (!main || !strip) return;

  var thumbs = Array.prototype.slice.call(strip.querySelectorAll('[data-full]'));
  if (!thumbs.length) return;

  function activate(btn) {
    var url = btn.getAttribute('data-full');
    if (!url) return;
    main.src = url;
    main.alt = btn.getAttribute('data-caption') || main.getAttribute('data-name') || '';
    thumbs.forEach(function (t) {
      var on = t === btn;
      t.classList.toggle('is-active', on);
      t.setAttribute('aria-current', on ? 'true' : 'false');
    });
  }

  strip.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-full]');
    if (btn) { e.preventDefault(); activate(btn); }
  });

  // Left/Right arrows move between thumbnails and preview as you go.
  strip.addEventListener('keydown', function (e) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    var idx = thumbs.indexOf(document.activeElement);
    if (idx === -1) return;
    var next = thumbs[e.key === 'ArrowRight' ? idx + 1 : idx - 1];
    if (next) { e.preventDefault(); next.focus(); activate(next); }
  });

  // Mark whichever thumbnail matches the current main image as active on load.
  var current = main.getAttribute('src');
  var match = thumbs.filter(function (t) { return t.getAttribute('data-full') === current; })[0];
  if (match) {
    match.classList.add('is-active');
    match.setAttribute('aria-current', 'true');
  }
}());
