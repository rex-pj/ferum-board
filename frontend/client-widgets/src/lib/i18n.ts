// Translation lookup for widgets.
//
// The server ships the `js-*` slice of the catalog to the browser as a
// `<meta name="ferum-i18n">` payload, which `ferum-utils.js` parses into
// `window.Ferum.t`. Widgets go through that same dictionary rather than
// carrying their own copy, so a string is translated once and both the
// vanilla page scripts and the custom elements pick it up.
//
// `ferum-utils.js` is a blocking <script> in <head> while the widget bundle is
// `defer`red, so `Ferum.t` is always defined by the time a widget renders. The
// fallback below only matters if a theme drops ferum-utils.js — returning the
// key keeps the UI greppable instead of blank.

type Translator = (key: string, args?: Record<string, string | number>) => string;

interface FerumGlobal {
  t?: Translator;
}

export function t(key: string, args?: Record<string, string | number>): string {
  const ferum = (window as unknown as { Ferum?: FerumGlobal }).Ferum;
  return ferum?.t ? ferum.t(key, args) : key;
}
