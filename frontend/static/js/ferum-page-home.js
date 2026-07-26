(function () {
  'use strict';

  /**
   * Alpine.js factory for the homepage product shelf's show-more control.
   *
   * WHAT IT DOES
   * At rest the shelf shows exactly one row; the control reveals the rest of the
   * fetched set. Overflow cards are hidden by INDEX, not by clipping the shelf to
   * a measured height. Height clipping was tried and is subtly wrong: the cut
   * lands on arithmetic (row height + paddings + gap, each of which changes per
   * breakpoint and carries subpixel error), so a sliver of the next row leaks
   * above the control whenever the sum is off by a few pixels. Hiding cards from
   * index `columns` onward has no arithmetic in it at all, so the row boundary is
   * exact everywhere by construction.
   *
   * THE ONE RULE THIS FILE FOLLOWS
   * Nothing measured here may depend on whether the shelf is currently open.
   * An earlier version counted the cards CSS had hidden, which is only meaningful
   * while collapsed — run it a moment too early during a collapse and the count
   * came back 0, which hid the control permanently. The count is now
   * `cards - columns`: a property of the layout, not of the state. That is what
   * makes it safe to hide cards with display:none again, which was the mechanism
   * blamed for that bug when the real culprit was measuring it.
   *
   * The column count is read from the grid's own computed grid-template-columns
   * rather than re-derived from breakpoints in JS, so the CSS stays the single
   * source of truth and retuning the grid needs no change here.
   */
  window.productShelf = function () {
    var SETTLE_DEBOUNCE_MS = 120;

    return {
      open: false,
      hiddenCount: 0,

      init: function () {
        var self = this;

        // $el.querySelector, NOT $refs. Alpine walks a component before its
        // children, so a child's x-ref is not reliably registered yet when the
        // parent's init() runs — that read came back undefined once and the
        // guard below returned, so the control silently never initialised.
        // $el is the component root and the document is fully parsed by the time
        // this deferred script executes, so a plain DOM query cannot be early.
        this.shelf = this.$el.querySelector('.fr-shelf');
        if (!this.shelf) return;

        // Three independent chances to land the first measurement, because
        // apply() is idempotent and this component has already shipped two
        // silent no-ops: once now, once after the tree settles (when
        // grid-template-columns has resolved to real track sizes rather than the
        // authored `repeat(...)`), and once from the ResizeObserver below, which
        // fires an initial observation as soon as it starts watching. Any one of
        // them succeeding is enough; the earlier ones bail harmlessly if the
        // layout is not ready.
        this.apply();
        this.$nextTick(function () { self.apply(); });

        var timer = null;
        var schedule = function () {
          window.clearTimeout(timer);
          timer = window.setTimeout(function () { self.apply(); }, SETTLE_DEBOUNCE_MS);
        };
        this._schedule = schedule;

        // The column count changes with the viewport. Observing the FIRST card
        // catches it, because a column change resizes every card — and card 0 is
        // never one of the hidden ones (index 0 is always below the column
        // count), so the observer can never be watching a display:none element.
        var first = this.shelf.querySelector('.fr-shelf-card');
        if (window.ResizeObserver && first) {
          this._ro = new window.ResizeObserver(schedule);
          this._ro.observe(first);
        }

        window.addEventListener('resize', schedule);
      },

      destroy: function () {
        if (this._schedule) window.removeEventListener('resize', this._schedule);
        if (this._ro) this._ro.disconnect();
      },

      /** Recompute from layout and write the result to the DOM. Idempotent. */
      apply: function () {
        var shelf = this.shelf;
        var cards = shelf.querySelectorAll('.fr-shelf-card');
        if (!cards.length) {
          this.hiddenCount = 0;
          return;
        }

        var tracks = window.getComputedStyle(shelf).gridTemplateColumns;

        // On an element the browser has not laid out yet this is still the
        // authored value ("repeat(5, minmax(0, 1fr))") or "none", which would
        // tokenise into a nonsense column count. Wait to be called back rather
        // than act on a guess — the observer always fires again.
        if (!tracks || tracks === 'none' || tracks.indexOf('repeat(') !== -1) return;

        var cols = (tracks.match(/\S+/g) || []).length || 1;
        this.hiddenCount = Math.max(0, cards.length - cols);

        // Collapsed: keep the first row's worth. Expanded: keep everything.
        var keep = this.open ? cards.length : cols;
        for (var i = 0; i < cards.length; i++) {
          cards[i].classList.toggle('fr-shelf-card--hidden', i >= keep);
        }
      },

      toggle: function () {
        this.open = !this.open;
        this.apply();
      },

      /** Collapsed-state label, e.g. "Show 7 more" — count is per-breakpoint. */
      moreLabel: function () {
        // Guarded: this runs inside x-text, and an exception there would take
        // the whole component down with it over a missing label.
        if (!window.Ferum || !window.Ferum.t) return '+' + this.hiddenCount;
        return window.Ferum.t('js-show-more-products', { count: this.hiddenCount });
      }
    };
  };
}());
