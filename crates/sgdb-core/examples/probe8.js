/**
 * Spike probe 8 — the S2 payoff.
 *
 * Probe 7 established that BPM renders into its **own document**, not SharedJSContext's, which
 * is why every detached-root mount was inert regardless of focus-system state.
 *
 * `g_PopupManager.m_mapPopups` maps popup names to popup objects that each carry their own
 * `window`/`document`. Because the **JS realm is shared**, Steam's React, the `Focusable`
 * factory, and the focus contexts are all still in hand — we just have to render into the
 * right document.
 *
 * This probe enumerates the popups, finds Big Picture's, and mounts there. Self-cleaning.
 */
(async () => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe8_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  // -- 1. Enumerate popups ----------------------------------------------------------------
  const popups = [];
  out.popups = (() => {
    const pm = window.g_PopupManager;
    if (!pm) return { error: 'g_PopupManager absent' };
    let entries = [];
    try {
      const m = pm.m_mapPopups;
      // Could be a Map or a plain object depending on build.
      entries = typeof m.entries === 'function' ? [...m.entries()] : Object.entries(m);
    } catch (e) {
      return { error: 'could not read m_mapPopups: ' + String(e) };
    }

    const described = entries.slice(0, 20).map(([name, popup]) => {
      const info = { name: String(name) };
      try {
        const win = popup && (popup.m_popup || popup.window || popup.BrowserWindow);
        const doc = win && win.document;
        info.keys = Object.keys(popup || {}).slice(0, 12);
        info.hasWindow = !!win;
        info.hasDocument = !!doc;
        info.title = doc ? String(doc.title).slice(0, 60) : null;
        info.bodyClass = doc && doc.body ? String(doc.body.className).slice(0, 120) : null;
        info.gamepadDom = doc ? !!doc.querySelector('[class*="gamepad" i]') : null;
        info.bodyChildren = doc && doc.body ? doc.body.children.length : null;
        if (doc) popups.push({ name: String(name), popup, win, doc, gamepad: info.gamepadDom });
      } catch (e) {
        info.error = String(e);
      }
      return info;
    });
    return described;
  })();

  // -- 2. Pick the Big Picture document ----------------------------------------------------
  const target =
    popups.find((p) => p.gamepad) ||
    popups.find((p) => /bigpicture|gamepad/i.test(p.name)) ||
    popups.find((p) => p.doc && p.doc.body && p.doc.body.children.length > 0);

  out.chosen = target
    ? { name: target.name, gamepadDom: target.gamepad, bodyChildren: target.doc.body.children.length }
    : null;

  if (!target) return { ...out, verdict: 'FAIL — no popup document with a rendered body found' };

  // -- 3. Mount into it --------------------------------------------------------------------
  out.mount = await (async () => {
    let container = null;
    let root = null;
    const doc = target.doc;
    try {
      const React = require('51745');
      const ReactDOM = require('98131');
      const Focusable = require('28869').HR('div');

      const wfc = window.g_WindowFocusCoordinator;
      const treesBefore = (() => { try { return wfc.m_rgTrees.length; } catch { return null; } })();

      // Create the element in the TARGET document, not ours — a node from a foreign document
      // cannot be appended.
      container = doc.createElement('div');
      container.id = 'sgdb-probe8-root';
      container.style.cssText =
        'position:fixed;right:0;bottom:0;width:2px;height:2px;opacity:0.01;z-index:-1;';
      doc.body.appendChild(container);

      let focusWithin = 0;
      root = ReactDOM.createRoot(container);
      root.render(
        React.createElement(
          Focusable,
          { focusableIfEmpty: true, noFocusRing: true, onFocusWithin: () => { focusWithin++; } },
          'probe',
        ),
      );
      await new Promise((r) => setTimeout(r, 600));

      const el = container.firstElementChild;
      if (!el) return { rendered: false };

      let treeNode = null;
      for (const k of Object.keys(el)) {
        try {
          const v = el[k];
          if (v && typeof v === 'object' && ('m_Tree' in v || 'm_rgChildren' in v)) {
            treeNode = { key: k, depth: v.m_nDepth ?? null, hasParent: !!v.m_Parent };
            break;
          }
        } catch {}
      }

      let focusResult;
      try {
        el.focus();
        await new Promise((r) => setTimeout(r, 250));
        focusResult = doc.activeElement === el ? 'FOCUSED' : 'did-not-stick';
      } catch (e) { focusResult = 'threw: ' + String(e); }
      try { if (doc.activeElement === el) el.blur(); } catch {}

      const treesAfter = (() => { try { return wfc.m_rgTrees.length; } catch { return null; } })();

      return {
        rendered: true,
        document: target.name,
        className: String(el.className),
        tabIndex: el.tabIndex,
        treeNode,
        focusResult,
        focusWithin,
        treesBefore,
        treesAfter,
      };
    } catch (e) {
      return { error: String((e && e.stack) || e) };
    } finally {
      try { root && root.unmount(); } catch {}
      try { container && container.remove(); } catch {}
    }
  })();

  const m = out.mount || {};
  out.verdict =
    m.rendered && m.treeNode && m.focusResult === 'FOCUSED'
      ? 'PASS — mounted into the BPM document, joined Steam focus tree, took focus'
      : m.rendered && (m.treeNode || m.focusResult === 'FOCUSED')
        ? 'PARTIAL — mounted into the BPM document with partial focus integration'
        : m.rendered
          ? 'RENDERED but not focus-integrated'
          : 'FAIL';
  return out;
})();
