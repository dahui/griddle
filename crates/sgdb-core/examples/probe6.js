/**
 * Spike probe 6 — S2, correctly constructed this time.
 *
 * Probe 4 failed because `28869.sl` is not a component: it is a props-splitting hook that
 * returns `{elemProps, navOptions, gamepadEvents}`. React rendered nothing because there was
 * nothing to render.
 *
 * Probe 5 found the real shape. Module 28869 exports a **component factory**:
 *
 *   HR = function L(e, t) { const r = S(e); return c.forwardRef((n, i) => I(e, r, n, i, t)); }
 *
 * i.e. Steam builds `Focusable` as `HR('div')`. It also exports three React contexts
 * (`Mg`, `TJ`, `sQ`) and there are two relevant globals:
 *
 *   FocusNavController      m_rgAllContexts / m_ActiveContext / m_rgGamepadInputSources
 *   g_WindowFocusCoordinator m_rgTrees / m_mapChildTreeCleanup
 *
 * Because the focus tree registry is a **global** rather than purely React context, a
 * detached root may still register. That is exactly what this probe measures: tree count and
 * context count before vs. after mounting.
 *
 * Self-cleaning: unmounts and removes the container in a `finally`.
 */
(async () => {
  const out = { steps: [] };
  const log = (step, detail) => out.steps.push({ step, ...detail });

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe6_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const snapshot = () => {
    const fnc = window.FocusNavController;
    const wfc = window.g_WindowFocusCoordinator;
    const len = (v) => {
      try {
        if (!v) return null;
        if (Array.isArray(v)) return v.length;
        if (v.size !== undefined) return v.size;
        if (typeof v === 'object') return Object.keys(v).length;
        return null;
      } catch { return null; }
    };
    return {
      allContexts: len(fnc && fnc.m_rgAllContexts),
      activeContext: !!(fnc && fnc.m_ActiveContext),
      trees: len(wfc && wfc.m_rgTrees),
      navigationSource: fnc ? String(fnc.m_navigationSource) : null,
    };
  };

  let container = null;
  let root = null;

  try {
    const React = require('51745');
    const ReactDOM = require('98131');
    const mod = require('28869');

    // Build Focusable the way Steam does: run the factory over an element type.
    let Focusable = null;
    let factoryError = null;
    try {
      Focusable = mod.HR('div');
    } catch (e) {
      factoryError = String(e);
    }

    log('factory', {
      factoryType: typeof mod.HR,
      built: !!Focusable,
      builtTypeof: Focusable ? String(Focusable.$$typeof) : null,
      isForwardRef: Focusable ? /forward_ref/.test(String(Focusable.$$typeof)) : false,
      error: factoryError,
    });

    if (!Focusable) return { ...out, verdict: 'FAIL — factory HR did not produce a component' };

    const before = snapshot();

    container = document.createElement('div');
    container.id = 'sgdb-probe6-root';
    // Focus-visible but user-invisible. display:none would make it unfocusable.
    container.style.cssText =
      'position:fixed;right:0;bottom:0;width:2px;height:2px;opacity:0.01;z-index:-1;';
    document.body.appendChild(container);

    let activated = 0;
    let focusWithinCalls = 0;

    root = ReactDOM.createRoot(container);
    root.render(
      React.createElement(
        Focusable,
        {
          onActivate: () => { activated++; },
          onFocusWithin: () => { focusWithinCalls++; },
          focusableIfEmpty: true,
          noFocusRing: true,
        },
        'probe',
      ),
    );

    await new Promise((r) => setTimeout(r, 500));

    const mounted = container.firstElementChild;
    const after = snapshot();

    log('mount', {
      rendered: !!mounted,
      tagName: mounted ? mounted.tagName : null,
      className: mounted ? String(mounted.className).slice(0, 120) : null,
      tabIndex: mounted ? mounted.tabIndex : null,
      html: String(container.innerHTML).slice(0, 200),
    });

    log('focusRegistry', {
      before,
      after,
      treesDelta:
        before.trees !== null && after.trees !== null ? after.trees - before.trees : null,
      contextsDelta:
        before.allContexts !== null && after.allContexts !== null
          ? after.allContexts - before.allContexts
          : null,
    });

    if (!mounted) return { ...out, verdict: 'FAIL — component built but rendered nothing' };

    // Is a Steam focus-tree node attached to the DOM element?
    let treeNode = null;
    for (const k of Object.keys(mounted)) {
      try {
        const v = mounted[k];
        if (v && typeof v === 'object' && ('m_Tree' in v || 'm_rgChildren' in v)) {
          treeNode = { key: k, depth: v.m_nDepth ?? null, hasParent: !!v.m_Parent };
          break;
        }
      } catch {}
    }
    log('treeNode', { found: !!treeNode, ...(treeNode || {}) });

    // Can it take focus?
    let focusResult = 'not-attempted';
    try {
      mounted.focus();
      await new Promise((r) => setTimeout(r, 200));
      focusResult = document.activeElement === mounted ? 'FOCUSED' : 'did-not-stick';
    } catch (e) {
      focusResult = 'threw: ' + String(e);
    }
    log('focus', { result: focusResult, focusWithinCalls, activated });

    try { if (document.activeElement === mounted) mounted.blur(); } catch {}

    const registered = !!treeNode || (after.trees ?? 0) > (before.trees ?? 0);
    const focused = focusResult === 'FOCUSED';
    out.verdict =
      registered && focused
        ? 'PASS — Focusable mounted, registered with Steam focus navigation, and took focus'
        : focused
          ? 'PARTIAL — renders and focuses, but no focus-tree registration detected'
          : registered
            ? 'PARTIAL — registered with focus navigation but focus() did not stick'
            : 'FAIL — renders but is not focus-integrated';
    return out;
  } catch (e) {
    out.verdict = 'ERROR: ' + String((e && e.stack) || e);
    return out;
  } finally {
    try { root && root.unmount(); } catch {}
    try { container && container.remove(); } catch {}
  }
})();
