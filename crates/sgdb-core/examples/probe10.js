/**
 * Spike probe 10 — the ModalManager itself, and Steam's own custom-artwork flow.
 *
 * Two targets:
 *
 * **Module 3673** carries `m_rgModals`, so it is the modal manager proper (module 36437's
 * export `L` merely *hosts* one, taking `{ModalManager, DialogWrapper, bUseDialogElement}`
 * as props).
 *
 * **Module 87498** contains BOTH `CloseModal` and `SetCustomArtworkForApp` — i.e. it is very
 * likely Steam's *own* "set custom artwork" dialog. If so it is the single most valuable
 * module in the bundle for this project: it demonstrates the exact call convention for the
 * apply API and the exact way Steam presents that UI, both of which we want to match rather
 * than reinvent.
 *
 * Inspection only. Opens nothing, applies nothing.
 */
(() => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe10_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const src = (id) => {
    try { return String(require.m[id]); } catch { return ''; }
  };

  const describe = (v) => {
    const t = typeof v;
    if (v === null) return { kind: 'null' };
    if (t === 'function') {
      const s = String(v);
      return {
        kind: /^class[\s{]/.test(s) ? 'class' : 'fn',
        name: String(v.name || ''),
        arity: v.length,
        protoKeys: (() => {
          try { return Object.getOwnPropertyNames(v.prototype || {}).slice(0, 25); } catch { return null; }
        })(),
        head: s.slice(0, 200).replace(/\s+/g, ' '),
      };
    }
    if (t === 'object') {
      return {
        kind: 'obj',
        reactTypeof: v.$$typeof ? String(v.$$typeof) : null,
        keys: (() => { try { return Object.keys(v).slice(0, 20); } catch { return null; } })(),
      };
    }
    return { kind: t };
  };

  const exportsOf = (id) => {
    try {
      const mod = require(id);
      const table = {};
      for (const k of Object.keys(mod)) {
        try { table[k] = describe(mod[k]); } catch (e) { table[k] = { error: String(e) }; }
      }
      return table;
    } catch (e) {
      return { error: String(e) };
    }
  };

  out.module3673 = exportsOf('3673');
  out.module87498 = exportsOf('87498');

  /** Text around each occurrence of `needle` in a module, whitespace collapsed. */
  const contextAround = (id, needle, before, after, max) => {
    const s = src(id);
    const hits = [];
    let idx = s.indexOf(needle);
    while (idx !== -1 && hits.length < max) {
      hits.push(s.slice(Math.max(0, idx - before), idx + after).replace(/\s+/g, ' '));
      idx = s.indexOf(needle, idx + 1);
    }
    return hits;
  };

  // How Steam itself calls the apply API — the convention we should match exactly.
  out.setCustomArtworkCallSites = contextAround('87498', 'SetCustomArtworkForApp', 400, 300, 4);
  out.clearCustomArtworkCallSites = contextAround('87498', 'ClearCustomArtworkForApp', 250, 250, 2);

  // The asset-type ordinals, whose enum members are mangled.
  out.assetTypeUsage = contextAround('87498', 'eAssetType', 200, 200, 4);

  // The modal manager's own shape.
  out.modalsContext = contextAround('3673', 'm_rgModals', 350, 350, 4);

  return out;
})();
