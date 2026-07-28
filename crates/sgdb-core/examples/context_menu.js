/**
 * S5 — the library context-menu entry point.
 *
 * The Decky plugin inserts "Change Artwork…" immediately before "Properties…". We want the
 * same, with a global-hotkey fallback if the anchor ever breaks.
 *
 * Four name-based guesses have already missed on this bundle, so nothing here is guessed.
 * The first pass narrowed it to:
 *
 *   module 5808   Properties + Uninstall + CreateDesktopShortcut + AddToFavorites + contextMenu
 *   module 39590  the only module containing lowercase `showContextMenu`
 *
 * This pass reads those two directly: the real localization tokens in menu order, and the
 * shape of the function that opens a menu. Tokens are content, so they survive minification —
 * the most durable anchor available.
 *
 * Read-only.
 */
(() => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_ctx_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const src = (id) => {
    try { return String(require.m[id]); } catch { return ''; }
  };

  // -- 1. Tokens in module 5808, in source order = menu order -------------------------------
  out.menuTokens = (() => {
    const s = src('5808');
    const toks = s.match(/#[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]+/g) || [];
    // De-dupe but keep first-seen order; that ordering is what tells us where to splice.
    const seen = new Set();
    const ordered = [];
    for (const t of toks) {
      if (seen.has(t)) continue;
      seen.add(t);
      ordered.push(t);
    }
    return { moduleSize: s.length, count: ordered.length, tokens: ordered.slice(0, 60) };
  })();

  // -- 2. Context around whichever token names Properties -----------------------------------
  // That is the splice point: our item goes immediately before it.
  out.propertiesSite = (() => {
    const s = src('5808');
    const hits = [];
    const re = /#[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]*Propert[A-Za-z0-9_]*/g;
    let m;
    while ((m = re.exec(s)) !== null && hits.length < 4) {
      hits.push({
        token: m[0],
        index: m.index,
        context: s.slice(Math.max(0, m.index - 320), m.index + 240).replace(/\s+/g, ' '),
      });
    }
    return hits;
  })();

  // -- 3. How menu items are constructed ------------------------------------------------------
  out.menuItemUsage = (() => {
    const s = src('5808');
    const hits = [];
    let idx = s.indexOf('MenuItem');
    while (idx !== -1 && hits.length < 5) {
      hits.push(s.slice(Math.max(0, idx - 180), idx + 220).replace(/\s+/g, ' '));
      idx = s.indexOf('MenuItem', idx + 1);
    }
    return hits;
  })();

  // -- 4. The exported surface of both modules -----------------------------------------------
  const describe = (id) => {
    try {
      const mod = require(id);
      const t = {};
      for (const k of Object.keys(mod)) {
        let v;
        try { v = mod[k]; } catch { continue; }
        t[k] = typeof v === 'function'
          ? { kind: /^class[\s{]/.test(String(v)) ? 'class' : 'fn', name: String(v.name || ''), arity: v.length,
              head: String(v).slice(0, 120).replace(/\s+/g, ' ') }
          : { kind: typeof v, reactTypeof: v && v.$$typeof ? String(v.$$typeof) : null };
      }
      return t;
    } catch (e) {
      return { error: String(e) };
    }
  };

  out.module5808 = describe('5808');
  out.module39590 = describe('39590');

  // -- 5. showContextMenu, the opener -----------------------------------------------------------
  out.showContextMenu = (() => {
    const s = src('39590');
    const idx = s.indexOf('showContextMenu');
    return idx === -1
      ? null
      : { context: s.slice(Math.max(0, idx - 300), idx + 400).replace(/\s+/g, ' ') };
  })();

  return out;
})();
