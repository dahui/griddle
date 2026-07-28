/**
 * Spike probe 7 — probe 6 re-run for Big Picture, plus the `showModal` call-site hunt.
 *
 * Probe 6 found that a detached React root renders `Focusable` but gets no focus integration.
 * That test ran in **desktop mode**, where `FocusNavController.m_ActiveContext` was falsy and
 * `g_WindowFocusCoordinator.m_rgTrees` was 0 — i.e. gamepad navigation may simply have been
 * dormant, which would make the negative result an artifact rather than a finding.
 *
 * This probe reports the focus subsystem's state in detail first, so the mount result can be
 * interpreted rather than guessed at.
 *
 * It also hunts `showModal` by **call site** rather than by name. Name-based searches gave two
 * false positives (an unrelated video-theater component; and `SettingsModalRoot`, which is a
 * CSS class string). Minified call sites look like `(0, n.XYZ)(...)`, so the reliable move is
 * to read the text around each literal `showModal` occurrence and see how Steam invokes it.
 *
 * Self-cleaning.
 */
(async () => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe7_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const ids = Object.keys(require.m);
  const sources = new Map();
  for (const id of ids) {
    try { sources.set(id, String(require.m[id])); } catch {}
  }

  // -- 1. Focus subsystem state, in detail ------------------------------------------------
  out.focusState = (() => {
    const fnc = window.FocusNavController;
    const wfc = window.g_WindowFocusCoordinator;
    if (!fnc) return { error: 'FocusNavController absent' };

    const ctxInfo = (c) => {
      if (!c) return null;
      try {
        return {
          name: String(c.m_strName || c.Name || ''),
          hasTree: !!c.m_Tree,
          active: !!c.m_bActive,
          keys: Object.keys(c).slice(0, 10),
        };
      } catch { return 'threw'; }
    };

    let contexts = [];
    try {
      const all = fnc.m_rgAllContexts;
      const arr = Array.isArray(all) ? all : all && all.length !== undefined ? [...all] : [];
      contexts = arr.slice(0, 8).map(ctxInfo);
    } catch {}

    return {
      allContextsCount: (() => { try { return fnc.m_rgAllContexts.length; } catch { return null; } })(),
      contexts,
      activeContext: ctxInfo(fnc.m_ActiveContext),
      lastActiveContext: ctxInfo(fnc.m_LastActiveContext),
      navigationSource: (() => { try { return JSON.stringify(fnc.m_navigationSource); } catch { return String(fnc.m_navigationSource); } })(),
      navigationSourceSupportsFocus: !!fnc.m_navigationSourceSupportsFocus,
      gamepadInputSources: (() => { try { return fnc.m_rgGamepadInputSources.length; } catch { return null; } })(),
      showDebugFocusRing: !!fnc.m_bShowDebugFocusRing,
      trees: (() => { try { return wfc.m_rgTrees.length; } catch { return null; } })(),
    };
  })();

  // -- 2. Which UI mode / windows are live ------------------------------------------------
  out.windows = (() => {
    try {
      const ws = window.SteamUIStore && window.SteamUIStore.m_WindowStore;
      if (!ws) return { error: 'no WindowStore' };
      const keys = Object.keys(ws);
      const out2 = { storeKeys: keys.slice(0, 20) };
      // GamepadUI window presence is the reliable BPM signal.
      for (const k of keys) {
        if (/gamepad|bigpicture|main/i.test(k)) {
          try {
            const v = ws[k];
            out2[k] = v ? { present: true, keys: Object.keys(v).slice(0, 8) } : { present: false };
          } catch { out2[k] = 'threw'; }
        }
      }
      out2.gamepadDom = !!document.querySelector('[class*="gamepad" i], [class*="Gamepad"]');
      out2.bodyClass = String(document.body.className).slice(0, 200);
      return out2;
    } catch (e) {
      return { error: String(e) };
    }
  })();

  // -- 3. Mount test, same as probe 6 ------------------------------------------------------
  out.mount = await (async () => {
    let container = null;
    let root = null;
    try {
      const React = require('51745');
      const ReactDOM = require('98131');
      const Focusable = require('28869').HR('div');

      const treesBefore = (() => { try { return window.g_WindowFocusCoordinator.m_rgTrees.length; } catch { return null; } })();

      container = document.createElement('div');
      container.style.cssText =
        'position:fixed;right:0;bottom:0;width:2px;height:2px;opacity:0.01;z-index:-1;';
      document.body.appendChild(container);

      root = ReactDOM.createRoot(container);
      root.render(React.createElement(Focusable, { focusableIfEmpty: true, noFocusRing: true }, 'probe'));
      await new Promise((r) => setTimeout(r, 500));

      const el = container.firstElementChild;
      if (!el) return { rendered: false };

      let treeNode = null;
      for (const k of Object.keys(el)) {
        try {
          const v = el[k];
          if (v && typeof v === 'object' && ('m_Tree' in v || 'm_rgChildren' in v)) {
            treeNode = { key: k, depth: v.m_nDepth ?? null };
            break;
          }
        } catch {}
      }

      let focusResult;
      try {
        el.focus();
        await new Promise((r) => setTimeout(r, 200));
        focusResult = document.activeElement === el ? 'FOCUSED' : 'did-not-stick';
      } catch (e) { focusResult = 'threw: ' + String(e); }
      try { if (document.activeElement === el) el.blur(); } catch {}

      const treesAfter = (() => { try { return window.g_WindowFocusCoordinator.m_rgTrees.length; } catch { return null; } })();

      return {
        rendered: true,
        className: String(el.className),
        tabIndex: el.tabIndex,
        treeNode,
        focusResult,
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

  // -- 4. showModal, by call site ----------------------------------------------------------
  out.showModalCallSites = (() => {
    const results = [];
    for (const [id, src] of sources) {
      let idx = src.indexOf('showModal');
      while (idx !== -1 && results.length < 12) {
        results.push({
          moduleId: id,
          // Enough context to see the import alias and the invocation shape.
          context: src.slice(Math.max(0, idx - 160), idx + 160).replace(/\s+/g, ' '),
        });
        idx = src.indexOf('showModal', idx + 1);
      }
      if (results.length >= 12) break;
    }
    return results;
  })();

  return out;
})();
