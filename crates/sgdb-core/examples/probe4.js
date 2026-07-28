/**
 * Spike probe 4 — the decisive S2 experiment.
 *
 * Probe 3 located the pieces:
 *   React 19.1.1        module 51745 (module-level export)
 *   ReactDOM createRoot module 98131
 *   Focusable           module 28869, export `sl`
 *                       (props: autoFocus / preferredFocus / noFocusRing / onFocusWithin /
 *                        navKey / fnCanTakeFocus / focusableIfEmpty / childFocusDisabled)
 *
 * The question this probe answers: **can we mount Steam's own `Focusable` from our own React
 * root and have it participate in Steam's focus navigation?** If yes, deliverable B is
 * viable. If no, we need to patch into Steam's existing tree instead, which is strictly more
 * fragile.
 *
 * # Cleanliness
 *
 * This mounts a real element into the live client. It is rendered 1x1 and effectively
 * invisible (opacity 0.01 — NOT `display:none`, which would make it unfocusable and
 * invalidate the test), and **it removes itself in a `finally`** so a thrown assertion cannot
 * leave debris in the user's Steam UI.
 */
(async () => {
  const out = { steps: [] };
  const log = (step, detail) => out.steps.push({ step, ...detail });

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe4_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  let container = null;
  let root = null;

  try {
    // -- Resolve the three pieces by the ids probe 3 found -------------------------------
    const React = require('51745');
    const ReactDOM = require('98131');
    const focusMod = require('28869');
    const Focusable = focusMod && focusMod.sl;

    log('resolve', {
      react: typeof React?.createElement === 'function' ? String(React.version) : 'MISSING',
      createRoot: typeof ReactDOM?.createRoot === 'function',
      focusable: typeof Focusable,
      focusableName: String(Focusable?.name || ''),
    });

    if (typeof React?.createElement !== 'function') return { ...out, verdict: 'no React' };
    if (typeof ReactDOM?.createRoot !== 'function') return { ...out, verdict: 'no createRoot' };
    if (typeof Focusable !== 'function') return { ...out, verdict: 'no Focusable' };

    // -- Mount ---------------------------------------------------------------------------
    container = document.createElement('div');
    container.id = 'sgdb-probe4-root';
    // Visible to the focus system, invisible to the user. display:none would be unfocusable.
    container.style.cssText =
      'position:fixed;right:0;bottom:0;width:1px;height:1px;opacity:0.01;pointer-events:none;z-index:-1;';
    document.body.appendChild(container);

    let activated = 0;
    let focusWithin = 0;

    root = ReactDOM.createRoot(container);
    root.render(
      React.createElement(
        Focusable,
        {
          // The full prop surface we would actually use, so this proves the real thing.
          onActivate: () => { activated++; },
          onFocusWithin: (b) => { if (b) focusWithin++; },
          noFocusRing: true,
          focusableIfEmpty: true,
          'data-sgdb-probe': '1',
        },
        'probe',
      ),
    );

    // Let React commit and Steam's focus tree register the node.
    await new Promise((r) => setTimeout(r, 400));

    const mounted = container.firstElementChild;
    log('mount', {
      rendered: !!mounted,
      tagName: mounted ? mounted.tagName : null,
      className: mounted ? String(mounted.className) : null,
      tabIndex: mounted ? mounted.tabIndex : null,
      html: String(container.innerHTML).slice(0, 200),
    });

    if (!mounted) return { ...out, verdict: 'FAIL — Focusable rendered nothing' };

    // -- Does Steam's focus system know about it? ------------------------------------------
    // Steam attaches its focus-tree node to the DOM element under a private key. Finding one
    // is the difference between "a div rendered" and "we are in the navigation tree".
    const ownKeys = Object.keys(mounted);
    const reactKeys = ownKeys.filter((k) => /^__react/.test(k));
    const focusNavKeys = ownKeys.filter((k) => !/^__react/.test(k));

    let treeNode = null;
    for (const k of ownKeys) {
      try {
        const v = mounted[k];
        if (v && typeof v === 'object' && ('m_Tree' in v || 'm_rgChildren' in v || 'm_Parent' in v)) {
          treeNode = {
            key: k,
            hasTree: 'm_Tree' in v,
            hasChildren: 'm_rgChildren' in v,
            hasParent: 'm_Parent' in v,
            depth: typeof v.m_nDepth === 'number' ? v.m_nDepth : null,
            parentIsSteams: !!(v.m_Parent && v.m_Parent !== v),
          };
          break;
        }
      } catch {}
    }

    log('focusTree', {
      ownKeyCount: ownKeys.length,
      reactKeys: reactKeys.slice(0, 3),
      otherKeys: focusNavKeys.slice(0, 8),
      treeNode,
    });

    // -- Can it actually take focus? -------------------------------------------------------
    const before = document.activeElement ? document.activeElement.tagName : null;
    let focusResult = 'not-attempted';
    try {
      mounted.focus();
      await new Promise((r) => setTimeout(r, 200));
      focusResult = document.activeElement === mounted ? 'FOCUSED' : 'focus() did not stick';
    } catch (e) {
      focusResult = 'threw: ' + String(e);
    }

    log('focus', {
      activeBefore: before,
      result: focusResult,
      activeAfter: document.activeElement ? document.activeElement.tagName : null,
      onFocusWithinFired: focusWithin,
    });

    // Restore focus so the user's client is left as we found it.
    try { if (document.activeElement === mounted) mounted.blur(); } catch {}

    const inTree = !!treeNode;
    const focused = focusResult === 'FOCUSED';
    out.verdict = inTree && focused
      ? 'PASS — mounted into Steam focus tree and took focus'
      : inTree
        ? 'PARTIAL — registered in focus tree but focus() did not stick'
        : focused
          ? 'PARTIAL — focusable DOM but no Steam focus-tree node found'
          : 'FAIL — rendered but not focus-integrated';
    out.activated = activated;
    return out;
  } catch (e) {
    out.verdict = 'ERROR: ' + String((e && e.stack) || e);
    return out;
  } finally {
    // Leave no debris in the live client, whatever happened above.
    try { root && root.unmount(); } catch {}
    try { container && container.remove(); } catch {}
  }
})();
