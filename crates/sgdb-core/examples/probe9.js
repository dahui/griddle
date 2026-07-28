/**
 * Spike probe 9 — inspect Steam's ModalManager (module 36437).
 *
 * Probe 8 established that rendering into BPM's document is not enough: `Focusable` needs
 * Steam's React **tree**, i.e. the module-28869 context providers. Steam's own modal system
 * mounts inside that tree, so it is the cleanest way in.
 *
 * **Inspection only — this opens nothing.** Three name-based searches have already produced
 * false positives on this bundle (a video-theater component, `SettingsModalRoot` which is a
 * CSS class string, and `HTMLDialogElement.showModal` itself). So: read the shapes first,
 * invoke afterwards.
 */
(() => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe9_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const ids = Object.keys(require.m);
  const sources = new Map();
  for (const id of ids) {
    try { sources.set(id, String(require.m[id])); } catch {}
  }

  const describe = (v) => {
    const t = typeof v;
    if (v === null) return { kind: 'null' };
    if (t === 'function') {
      const src = String(v);
      return {
        kind: /^class[\s{]/.test(src) ? 'class' : 'fn',
        name: String(v.name || ''),
        arity: v.length,
        head: src.slice(0, 160).replace(/\s+/g, ' '),
        // Instance methods, for a class.
        protoKeys: (() => {
          try { return Object.getOwnPropertyNames(v.prototype || {}).slice(0, 20); } catch { return null; }
        })(),
      };
    }
    if (t === 'object') {
      return {
        kind: 'obj',
        reactTypeof: v.$$typeof ? String(v.$$typeof) : null,
        keys: (() => { try { return Object.keys(v).slice(0, 20); } catch { return null; } })(),
        // A live ModalManager instance would carry state like a modal stack.
        ownState: (() => {
          try {
            return Object.keys(v).filter((k) => /^m_/.test(k)).slice(0, 15);
          } catch { return null; }
        })(),
      };
    }
    return { kind: t, value: String(v).slice(0, 60) };
  };

  // -- 1. Module 36437's exports -----------------------------------------------------------
  out.module36437 = (() => {
    try {
      const mod = require('36437');
      const table = {};
      for (const k of Object.keys(mod)) {
        try { table[k] = describe(mod[k]); } catch (e) { table[k] = { error: String(e) }; }
      }
      return table;
    } catch (e) {
      return { error: String(e) };
    }
  })();

  // -- 2. The ModalManager text in context --------------------------------------------------
  out.modalManagerContext = (() => {
    const src = sources.get('36437') || '';
    const hits = [];
    let idx = src.indexOf('ModalManag');
    while (idx !== -1 && hits.length < 6) {
      hits.push(src.slice(Math.max(0, idx - 300), idx + 400).replace(/\s+/g, ' '));
      idx = src.indexOf('ModalManag', idx + 1);
    }
    return hits;
  })();

  // -- 3. Any module that looks like it *opens* modals ---------------------------------------
  // Search by behaviour: a modal stack plus a push/close pair.
  out.modalStackModules = (() => {
    const found = [];
    for (const [id, src] of sources) {
      if (!/m_rgModals|ModalStack|rgModalStack|OpenModal|CloseModal/.test(src)) continue;
      const m = src.match(/(m_rgModals|ModalStack|rgModalStack|OpenModal|CloseModal)/g) || [];
      found.push({ moduleId: id, markers: [...new Set(m)].slice(0, 6) });
      if (found.length >= 10) break;
    }
    return found;
  })();

  // -- 4. Globals that might already hold a modal/router surface -----------------------------
  out.globalSurfaces = (() => {
    const res = {};
    for (const name of ['SteamUIStore', 'g_PopupManager', 'MainWindowBrowserManager']) {
      try {
        const v = window[name];
        if (!v) { res[name] = 'absent'; continue; }
        const keys = Object.keys(v);
        res[name] = {
          methods: keys.filter((k) => {
            try { return typeof v[k] === 'function'; } catch { return false; }
          }).slice(0, 25),
          modalish: keys.filter((k) => /modal|dialog|popup|route|nav/i.test(k)).slice(0, 20),
        };
        // Prototype methods matter for class instances (own keys are just state).
        const proto = Object.getPrototypeOf(v);
        res[name].protoMethods = proto
          ? Object.getOwnPropertyNames(proto)
              .filter((k) => /modal|dialog|show|open|nav|route/i.test(k))
              .slice(0, 25)
          : null;
      } catch (e) {
        res[name] = 'threw: ' + String(e);
      }
    }
    return res;
  })();

  return out;
})();
