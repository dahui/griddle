/**
 * Follow-up spike probe. Three questions the first pass left open:
 *
 * 1. Is the `Focusable` export we found actually a usable React component?
 * 2. What are the REAL localization-token anchors for the library context menu? The first
 *    pass guessed `#AppProperties_Title` and `#AppDetails_Properties`; both scored zero, so
 *    the guesses were wrong — this searches for what is actually there. (S5)
 * 3. Where does the asset-type enum live, given its members are mangled? (Needed to pass the
 *    right ordinal to SetCustomArtworkForApp.)
 *
 * Read-only: reads module source, executes only a narrow candidate set, calls no Set* API.
 */
(() => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_probe2_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const sources = new Map();
  for (const id of Object.keys(require.m)) {
    try { sources.set(id, String(require.m[id])); } catch {}
  }

  const grep = (re) => {
    const hits = [];
    for (const [id, src] of sources) if (re.test(src)) hits.push(id);
    return hits;
  };

  // -- 1. Inspect the Focusable candidate ------------------------------------------------
  out.focusable = (() => {
    try {
      const mod = require('4690');
      const val = mod && mod.Bp;
      if (!val) return { found: false };
      return {
        found: true,
        type: typeof val,
        isReactComponent:
          typeof val === 'object' && val !== null && '$$typeof' in val
            ? String(val.$$typeof)
            : typeof val === 'function'
              ? 'function-component'
              : 'unknown',
        displayName: String((val && (val.displayName || val.name)) || ''),
        // A forwardRef/memo wrapper exposes its inner render fn; its source tells us whether
        // this is the real focus-navigation component or just something that mentions it.
        sourceHead: String((val && val.render) || val).slice(0, 300),
        // Other exports of the same module, to see what else is co-located.
        siblingKeys: Object.keys(mod).slice(0, 40),
      };
    } catch (e) {
      return { found: false, error: String(e) };
    }
  })();

  // -- 2. Real localization tokens for the context menu (S5) -----------------------------
  // Steam's tokens are `#Namespace_Key`. Collect every token that looks context-menu-ish
  // rather than guessing individual names.
  out.tokens = (() => {
    const found = new Map();
    const re = /#[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]+/g;
    const interesting =
      /(Properties|ContextMenu|Manage|LibraryContext|CreateShortcut|AddToFavorites|ManageGame|Uninstall)/i;
    for (const [id, src] of sources) {
      const m = src.match(re);
      if (!m) continue;
      for (const tok of m) {
        if (!interesting.test(tok)) continue;
        if (!found.has(tok)) found.set(tok, id);
      }
    }
    return Object.fromEntries([...found.entries()].slice(0, 60));
  })();

  // -- 3. Modules that build the app context menu ---------------------------------------
  out.contextMenuModules = {
    // The menu is assembled somewhere that also references Properties + Uninstall.
    propertiesAndUninstall: grep(/Properties/).filter((id) => /Uninstall/.test(sources.get(id) || '')).slice(0, 10),
    menuItemFactory: grep(/MenuItem/).slice(0, 10),
    showContextMenu: grep(/showContextMenu|ShowContextMenu/).slice(0, 10),
  };

  // -- 4. The asset type enum ------------------------------------------------------------
  out.assetTypeEnum = (() => {
    // Look for the enum shape: an object literal assigning 0..5 to five or six mangled keys,
    // in a module that also knows about custom artwork.
    const candidates = grep(/SetCustomArtworkForApp/);
    const results = [];
    for (const id of candidates.slice(0, 10)) {
      const src = sources.get(id) || '';
      // Capture the surrounding text so the enum can be identified by eye.
      const idx = src.indexOf('SetCustomArtworkForApp');
      results.push({
        moduleId: id,
        context: src.slice(Math.max(0, idx - 300), idx + 200),
      });
    }
    return results;
  })();

  return out;
})();
