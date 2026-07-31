/**
 * Spike probe 18 — confirm focus-tree membership **positively**.
 *
 * Probe 17 invalidated every previous focus check. Steam's *own* `Focusable` elements carry no
 * focus node on the DOM element either — `Object.getOwnPropertyNames` shows 2 properties (the
 * React fiber keys) and nothing else, exactly like ours. The tree entries are
 * `{name, tree, browserContext}`, so membership lives inside `tree`, not on elements.
 *
 * Every "treeNode: null" from probes 6-16 was therefore measuring nothing. Absence of evidence
 * produced by a broken test is not evidence of absence.
 *
 * This probe searches the actual tree structure for our element, and compares against a
 * Steam-rendered control so a null result can be interpreted rather than guessed at.
 */
(async () => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe18_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }

  const docs = [{ name: 'self', doc: document }];
  try {
    const m = window.g_PopupManager && window.g_PopupManager.m_mapPopups;
    const entries = m ? (typeof m.entries === 'function' ? [...m.entries()] : Object.entries(m)) : [];
    for (const [name, popup] of entries) {
      const w = popup && (popup.m_popup || popup.window);
      if (w && w.document) docs.push({ name: String(name), doc: w.document });
    }
  } catch {}

  // -- Tree structure, so membership can be searched ---------------------------------------
  out.trees = (() => {
    try {
      const trees = Array.from(window.g_WindowFocusCoordinator.m_rgTrees);
      return trees.map((entry, i) => {
        const t = entry.tree;
        return {
          i,
          name: String(entry.name || ''),
          treeKeys: t ? Object.keys(t).slice(0, 16) : null,
          rootKeys: (() => {
            try {
              const r = t.m_Root || t.Root || t.m_RootNode;
              return r ? Object.keys(r).slice(0, 16) : null;
            } catch { return null; }
          })(),
        };
      });
    } catch (e) {
      return { error: String(e) };
    }
  })();

  /** Walk a focus tree collecting every node's backing DOM element. */
  const collectElements = (tree) => {
    const els = [];
    const seen = new WeakSet();
    const visit = (node, depth) => {
      if (!node || typeof node !== 'object' || depth > 60 || els.length > 4000) return;
      if (seen.has(node)) return;
      seen.add(node);
      try {
        const el = node.m_element || node.element || node.m_elem;
        if (el && el.nodeType === 1) els.push({ el, depth });
      } catch {}
      try {
        const kids = node.m_rgChildren || node.children;
        if (kids && typeof kids.length === 'number') {
          for (let i = 0; i < kids.length; i++) visit(kids[i], depth + 1);
        }
      } catch {}
    };
    try {
      const root = tree.m_Root || tree.Root || tree.m_RootNode || tree;
      visit(root, 0);
    } catch {}
    return els;
  };

  // -- Open our modal via the inline manager ------------------------------------------------
  const React = require('51745');
  const Focusable = require('28869').HR('div');

  const fiberKeyOf = (el) => {
    for (const k of Object.keys(el)) if (k.startsWith('__reactFiber$')) return k;
    return null;
  };
  let mgr = null;
  for (const d of docs) {
    let el = null, key = null;
    try {
      const all = d.doc.querySelectorAll('*');
      for (let i = 0; i < all.length && i < 500; i++) {
        const k = fiberKeyOf(all[i]);
        if (k) { el = all[i]; key = k; break; }
      }
    } catch {}
    if (!el) continue;
    let root = el[key];
    while (root && root.return) root = root.return;
    let visited = 0;
    const visit = (f, depth) => {
      if (!f || mgr || visited > 60000) return;
      visited++;
      try {
        const p = f.memoizedProps;
        if (p && typeof p === 'object') {
          for (const k2 of Object.keys(p)) {
            if (!/modalmanager/i.test(k2)) continue;
            const v = p[k2];
            if (v && typeof v === 'object' && typeof v.ShowModal === 'function') {
              let usePopups = true;
              try { usePopups = !!v.BUsePopups(); } catch {}
              if (!usePopups) { mgr = v; return; }
            }
          }
        }
      } catch {}
      visit(f.child, depth + 1);
      visit(f.sibling, depth);
    };
    try { visit(root, 0); } catch {}
    if (mgr) break;
  }
  if (!mgr) return { ...out, verdict: 'FAIL — no inline manager' };

  let handle = null;
  try {
    const Body = () =>
      React.createElement(
        Focusable,
        { focusableIfEmpty: true, autoFocus: true, 'data-sgdb-probe': 'p18',
          style: { padding: '2rem', background: '#1b1d27', color: '#e6e8f0' } },
        'probe 18',
      );
    handle = mgr.ShowModal(React.createElement(Body));
    await new Promise((r) => setTimeout(r, 1400));

    let ours = null, where = null;
    for (const d of docs) {
      try {
        const q = d.doc.querySelector('[data-sgdb-probe="p18"]');
        if (q) { ours = q; where = d.name; break; }
      } catch {}
    }

    // A Steam-rendered control from the same document.
    let control = null;
    try {
      const d = docs.find((x) => x.name === where);
      const list = d ? d.doc.querySelectorAll('.Focusable') : [];
      for (const c of list) { if (c !== ours && !c.contains(ours)) { control = c; break; } }
    } catch {}

    const membership = (() => {
      try {
        const trees = Array.from(window.g_WindowFocusCoordinator.m_rgTrees);
        const res = [];
        for (let i = 0; i < trees.length; i++) {
          const els = collectElements(trees[i].tree);
          res.push({
            tree: i,
            name: String(trees[i].name || ''),
            nodeCount: els.length,
            containsOurs: !!ours && els.some((e) => e.el === ours || e.el.contains(ours)),
            containsControl: !!control && els.some((e) => e.el === control || e.el.contains(control)),
          });
        }
        return res;
      } catch (e) {
        return { error: String(e) };
      }
    })();

    out.result = {
      ourElementFound: !!ours,
      inDocument: where,
      controlFound: !!control,
      controlClass: control ? String(control.className).slice(0, 50) : null,
      membership,
    };

    const oursIn = Array.isArray(membership) && membership.some((m) => m.containsOurs);
    const ctrlIn = Array.isArray(membership) && membership.some((m) => m.containsControl);
    const anyNodes = Array.isArray(membership) && membership.some((m) => m.nodeCount > 0);

    out.verdict = !anyNodes
      ? 'INCONCLUSIVE — could not walk the tree structure (neither ours nor Steam control found in it)'
      : oursIn && ctrlIn
        ? 'PASS — our Focusable is in the same focus tree as Steam-rendered controls'
        : ctrlIn && !oursIn
          ? 'FAIL — Steam controls are in the tree, ours is not'
          : `ODD — ours=${oursIn} control=${ctrlIn}`;
    return out;
  } catch (e) {
    return { ...out, verdict: 'ERROR: ' + String((e && e.message) || e).slice(0, 200) };
  } finally {
    try { if (handle && handle.Close) handle.Close(); } catch {}
  }
})();
