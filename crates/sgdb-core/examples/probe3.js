/**
 * Spike probe 3 — the second half of S2.
 *
 * Locating Steam's focus-tree class (probe 1) is not the same as being able to *mount* our
 * own UI into Steam's React tree and have it participate in gamepad focus navigation. This
 * probe finds the pieces that mounting requires:
 *
 *   1. React itself (we must use Steam's instance — a second React would not share context)
 *   2. ReactDOM (createRoot / render)
 *   3. The `Focusable` React component, as distinct from the focus-tree class
 *   4. `showModal` + `ModalRoot`, the entry point the plan chose over patching Steam's router
 *
 * Read-only: reads module source, executes only narrow candidate sets, mounts nothing yet.
 */
(() => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe3_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const ids = Object.keys(require.m);
  const sources = new Map();
  for (const id of ids) {
    try { sources.set(id, String(require.m[id])); } catch {}
  }

  /** Run a module and hand its exports to `pick`; returns the first non-null result. */
  const findExport = (candidateIds, pick, limit = 60) => {
    for (const id of candidateIds.slice(0, limit)) {
      let mod;
      try { mod = require(id); } catch { continue; }
      if (!mod || (typeof mod !== 'object' && typeof mod !== 'function')) continue;
      let keys;
      try { keys = Object.keys(mod); } catch { continue; }
      for (const key of keys) {
        let val;
        try { val = mod[key]; } catch { continue; }
        try {
          const hit = pick(val, key, mod, id);
          if (hit) return { moduleId: id, exportKey: key, ...hit };
        } catch {}
      }
    }
    return null;
  };

  const grep = (re) => ids.filter((id) => re.test(sources.get(id) || ''));

  const describe = (val) => {
    if (val == null) return 'null';
    const t = typeof val;
    if (t !== 'object' && t !== 'function') return t;
    const sym = val.$$typeof ? String(val.$$typeof) : null;
    return {
      type: t,
      reactTypeof: sym,
      name: String(val.displayName || val.name || ''),
      keys: t === 'object' ? Object.keys(val).slice(0, 12) : undefined,
    };
  };

  // -- 1. React ---------------------------------------------------------------------------
  out.react = (() => {
    const hit = findExport(
      grep(/createElement/),
      (val) =>
        val &&
        typeof val === 'object' &&
        typeof val.createElement === 'function' &&
        typeof val.useState === 'function' &&
        typeof val.Fragment !== 'undefined'
          ? { version: String(val.version || 'unknown'), via: 'namespace-export' }
          : null,
    );
    if (hit) return hit;

    // React is often the module's default export rather than a named one.
    for (const id of grep(/createElement/).slice(0, 80)) {
      let mod;
      try { mod = require(id); } catch { continue; }
      if (mod && typeof mod.createElement === 'function' && typeof mod.useState === 'function') {
        return { moduleId: id, exportKey: '<module>', version: String(mod.version || 'unknown') };
      }
    }
    return null;
  })();

  // -- 2. ReactDOM ------------------------------------------------------------------------
  out.reactDom = (() => {
    for (const id of grep(/createRoot|unstable_createRoot/).slice(0, 60)) {
      let mod;
      try { mod = require(id); } catch { continue; }
      if (!mod) continue;
      const hasCreateRoot = typeof mod.createRoot === 'function';
      const hasRender = typeof mod.render === 'function';
      if (hasCreateRoot || hasRender) {
        return {
          moduleId: id,
          createRoot: hasCreateRoot,
          render: hasRender,
          version: String(mod.version || 'unknown'),
        };
      }
    }
    return null;
  })();

  // -- 3. The Focusable React component ---------------------------------------------------
  // Distinct from the focus-tree class in module 4690. A React component here is usually a
  // forwardRef object, so match on $$typeof rather than on `typeof === 'function'`.
  out.focusableComponent = (() => {
    const candidates = grep(/Focusable/);
    const found = [];
    for (const id of candidates.slice(0, 40)) {
      let mod;
      try { mod = require(id); } catch { continue; }
      if (!mod || typeof mod !== 'object') continue;
      for (const key of Object.keys(mod)) {
        let val;
        try { val = mod[key]; } catch { continue; }
        if (!val) continue;
        const isForwardRef =
          typeof val === 'object' && val.$$typeof && /forward_ref|memo/.test(String(val.$$typeof));
        const isFn = typeof val === 'function' && !/^class /.test(String(val).slice(0, 8));
        if (!isForwardRef && !isFn) continue;
        const src = String((val && val.render) || val).slice(0, 600);
        if (/Focusable|focusable|m_Tree|FocusRing/.test(src)) {
          found.push({ moduleId: id, exportKey: key, info: describe(val), sourceHead: src.slice(0, 220) });
          if (found.length >= 6) break;
        }
      }
      if (found.length >= 6) break;
    }
    return found;
  })();

  // -- 4. Module 4690's full export table -------------------------------------------------
  // Probe 1 found the focus-tree class here; list everything so the React component and the
  // tree accessor can be told apart.
  out.module4690 = (() => {
    try {
      const mod = require('4690');
      const table = {};
      for (const key of Object.keys(mod)) {
        try { table[key] = describe(mod[key]); } catch { table[key] = 'threw'; }
      }
      return table;
    } catch (e) {
      return { error: String(e) };
    }
  })();

  // -- 5. showModal / ModalRoot ------------------------------------------------------------
  out.modal = (() => {
    const result = { showModal: null, modalRoot: null };
    result.showModal = findExport(
      grep(/showModal/),
      (val, key) =>
        typeof val === 'function' && /^showModal$/i.test(key)
          ? { arity: val.length, sourceHead: String(val).slice(0, 200) }
          : null,
    );
    if (!result.showModal) {
      // Name may be mangled; fall back to any function whose source mentions modal mounting.
      result.showModal = findExport(grep(/showModal/), (val) => {
        if (typeof val !== 'function') return null;
        const src = String(val);
        return /showModal|ModalManager|CreatePopup/.test(src) && src.length < 4000
          ? { arity: val.length, sourceHead: src.slice(0, 200) }
          : null;
      });
    }
    result.modalRoot = findExport(grep(/ModalRoot/), (val, key) =>
      /ModalRoot/.test(key) ? { info: describe(val) } : null,
    );
    return result;
  })();

  // -- 6. Is Big Picture currently open? ---------------------------------------------------
  // Mounting can be proven in desktop mode, but gamepad *input* needs BPM, so report which
  // mode we are looking at.
  out.uiMode = (() => {
    try {
      const store = window.SteamUIStore;
      const modeNames = { 0: 'Desktop/Unknown', 4: 'GamepadUI (Big Picture)', 7: 'GamepadUI' };
      const raw = store && (store.GetCurrentUIMode ? store.GetCurrentUIMode() : store.CurrentUIMode);
      return {
        raw: raw === undefined ? 'unavailable' : String(raw),
        guess: modeNames[raw] || 'unknown',
        gamepadDomPresent: !!document.querySelector('[class*="gamepad" i]'),
        windowCount: (() => {
          try { return Object.keys(store.WindowStore || {}).length; } catch { return -1; }
        })(),
      };
    } catch (e) {
      return { error: String(e) };
    }
  })();

  return out;
})();
