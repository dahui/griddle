/**
 * Spike probe 5 — diagnose probe 4's failure and find the real mount point.
 *
 * Probe 4 mounted Steam's `Focusable` into a **detached** React root and it rendered nothing.
 * Two candidate explanations, and they lead to very different architectures:
 *
 *   (a) our root is broken — then nothing works and B is in trouble;
 *   (b) `Focusable` returns null without focus-tree context from a parent provider — then we
 *       must mount inside Steam's existing tree, which is what `showModal` is for.
 *
 * (b) would confirm the plan's choice of a modal over a route, so it matters which it is.
 *
 * This probe: renders a plain div as a control, then hunts the modal system and the focus
 * context provider. Self-cleaning.
 */
(async () => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe5_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const ids = Object.keys(require.m);
  const sources = new Map();
  for (const id of ids) {
    try { sources.set(id, String(require.m[id])); } catch {}
  }
  const grep = (re) => ids.filter((id) => re.test(sources.get(id) || ''));

  const React = require('51745');
  const ReactDOM = require('98131');

  // -- Control: does a detached root render a plain div at all? ---------------------------
  out.controlRender = await (async () => {
    let container = null;
    let root = null;
    try {
      container = document.createElement('div');
      container.style.cssText = 'position:fixed;right:0;bottom:0;width:1px;height:1px;opacity:0.01;';
      document.body.appendChild(container);
      root = ReactDOM.createRoot(container);
      root.render(React.createElement('div', { id: 'sgdb-control' }, 'x'));
      await new Promise((r) => setTimeout(r, 300));
      return {
        rendered: !!container.firstElementChild,
        html: String(container.innerHTML).slice(0, 120),
      };
    } catch (e) {
      return { error: String(e) };
    } finally {
      try { root && root.unmount(); } catch {}
      try { container && container.remove(); } catch {}
    }
  })();

  // -- What does Focusable actually do when it bails? -------------------------------------
  out.focusableSource = (() => {
    try {
      const fn = require('28869').sl;
      // The whole function, so the early-return condition is visible.
      return String(fn).slice(0, 1400);
    } catch (e) {
      return 'error: ' + String(e);
    }
  })();

  // -- Module 28869's full export table ---------------------------------------------------
  // The focus context provider is most likely a sibling of the Focusable component.
  out.module28869 = (() => {
    try {
      const mod = require('28869');
      const table = {};
      for (const key of Object.keys(mod)) {
        let v;
        try { v = mod[key]; } catch { table[key] = 'threw'; continue; }
        const t = typeof v;
        table[key] =
          t === 'function'
            ? { kind: 'fn', name: String(v.name || ''), head: String(v).slice(0, 90) }
            : t === 'object' && v
              ? {
                  kind: 'obj',
                  reactTypeof: v.$$typeof ? String(v.$$typeof) : null,
                  keys: Object.keys(v).slice(0, 8),
                }
              : { kind: t };
      }
      return table;
    } catch (e) {
      return { error: String(e) };
    }
  })();

  // -- Hunt the modal system --------------------------------------------------------------
  // Look for the function Steam itself calls to open a modal. Signature in decky-frontend-lib
  // is showModal(modal, parent?, props?) returning { Close() }.
  out.modalHunt = (() => {
    const hits = [];
    // Call sites first: find how Steam invokes it, which names the export.
    const callSites = [];
    for (const [id, src] of sources) {
      const m = src.match(/(\w+)\.(\w+)\)?\(\s*\(0,\w+\.jsx\)/g);
      if (m && /Modal/.test(src)) callSites.push({ id, sample: m.slice(0, 2) });
      if (callSites.length >= 6) break;
    }

    const modalModules = grep(/m_rgModalStack|ModalManager|CreateModal|showModal/);
    for (const id of modalModules.slice(0, 25)) {
      let mod;
      try { mod = require(id); } catch { continue; }
      if (!mod || typeof mod !== 'object') continue;
      for (const key of Object.keys(mod)) {
        let v;
        try { v = mod[key]; } catch { continue; }
        if (typeof v !== 'function') continue;
        const src = String(v);
        // The real showModal creates a popup/browser window or pushes onto a modal stack,
        // and hands back something with a Close method.
        if (/Close\s*[:(]/.test(src) && /[Mm]odal/.test(src) && src.length < 3000) {
          hits.push({ moduleId: id, exportKey: key, name: String(v.name || ''), arity: v.length, head: src.slice(0, 220) });
          if (hits.length >= 8) break;
        }
      }
      if (hits.length >= 8) break;
    }
    return { callSites: callSites.slice(0, 4), candidates: hits };
  })();

  // -- Globals that might expose the modal / popup manager --------------------------------
  out.globals = (() => {
    const interesting = {};
    for (const k of Object.getOwnPropertyNames(window)) {
      if (!/popup|modal|steamui|router|focus/i.test(k)) continue;
      try {
        const v = window[k];
        interesting[k] = v && typeof v === 'object'
          ? { type: 'object', keys: Object.keys(v).slice(0, 14) }
          : typeof v;
      } catch {
        interesting[k] = 'threw';
      }
    }
    return interesting;
  })();

  return out;
})();
