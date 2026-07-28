/**
 * M1 spike probe. Evaluated inside Steam's SharedJSContext via CDP Runtime.evaluate.
 *
 * Answers S1 (are we really in Steam's realm), S2 (can we reach the webpack module registry
 * and find gamepad-focusable components), S6 (what does the CSP allow), and the feature
 * detection that gates live apply.
 *
 * # Safety note on module enumeration
 *
 * Decky and Millennium discover modules by *executing* every webpack factory and inspecting
 * the resulting exports. That works, but in a live client it runs a few thousand module
 * initializers for their side effects.
 *
 * This probe reads `require.m[id].toString()` instead, which returns each factory's source
 * **without running it**. Candidates are found by source text, and only the handful that
 * match are then executed. Same information, far smaller blast radius — and it also happens
 * to be much faster.
 *
 * Returns a JSON-serializable object. Never throws: every section is independently
 * try/caught, because a probe that dies halfway tells you less than one that reports a
 * partial result.
 */
(() => {
  const out = { ok: true, sections: {} };

  const section = (name, fn) => {
    try {
      out.sections[name] = fn();
    } catch (e) {
      out.sections[name] = { error: String((e && e.message) || e) };
    }
  };

  // -- S1: are we actually in Steam's shared realm? -------------------------------------
  section('realm', () => ({
    href: String(location.href),
    title: String(document.title),
    // Valve declares `var CLSTAMP="10840511"` on line 1 of steamui/library.js.
    clstamp: typeof CLSTAMP === 'string' ? CLSTAMP : null,
    hasSteamClient: typeof SteamClient === 'object' && SteamClient !== null,
    steamClientKeys: typeof SteamClient === 'object' && SteamClient
      ? Object.keys(SteamClient).length
      : 0,
    hasAppStore: typeof window.appStore === 'object' && window.appStore !== null,
    hasAppDetailsStore: typeof window.appDetailsStore === 'object' && window.appDetailsStore !== null,
    hasCollectionStore: typeof window.collectionStore === 'object' && window.collectionStore !== null,
    hasUIStore: typeof window.SteamUIStore === 'object' && window.SteamUIStore !== null,
  }));

  // -- Feature detection for the live-apply path ----------------------------------------
  section('applyApi', () => {
    const apps = (typeof SteamClient === 'object' && SteamClient && SteamClient.Apps) || null;
    const sig = (fn) => (typeof fn === 'function' ? fn.length : null);
    return {
      present: !!apps,
      SetCustomArtworkForApp: sig(apps && apps.SetCustomArtworkForApp),
      ClearCustomArtworkForApp: sig(apps && apps.ClearCustomArtworkForApp),
      SetCustomLogoPositionForApp: sig(apps && apps.SetCustomLogoPositionForApp),
      ClearCustomLogoPositionForApp: sig(apps && apps.ClearCustomLogoPositionForApp),
      // Present on Deck; worth knowing whether desktop has it too.
      ReportLibraryAssetCacheMiss: sig(apps && apps.ReportLibraryAssetCacheMiss),
    };
  });

  // -- S2: the webpack module registry --------------------------------------------------
  section('webpack', () => {
    const chunk = window.webpackChunksteamui;
    if (!Array.isArray(chunk)) {
      return { hasChunkArray: false, note: 'webpackChunksteamui absent — S2 approach unavailable' };
    }

    let require = null;
    const marker = 'sgdb_probe_' + Math.random().toString(36).slice(2);
    try {
      chunk.push([[marker], {}, (r) => { require = r; }]);
    } catch (e) {
      return { hasChunkArray: true, captured: false, error: String(e) };
    }

    if (!require || typeof require.m !== 'object') {
      return { hasChunkArray: true, captured: false, note: 'push succeeded but no require.m' };
    }

    const ids = Object.keys(require.m);

    // Read factory SOURCE without executing. `Function.prototype.toString` on a webpack
    // factory gives the module's compiled body.
    const sources = new Map();
    let unreadable = 0;
    for (const id of ids) {
      try {
        sources.set(id, String(require.m[id]));
      } catch {
        unreadable++;
      }
    }

    /** Module ids whose source matches every supplied needle. */
    const grep = (...needles) => {
      const hits = [];
      for (const [id, src] of sources) {
        if (needles.every((n) => src.includes(n))) hits.push(id);
      }
      return hits;
    };

    // Structural / content anchors. Localization tokens (`#Foo_Bar`) are content rather than
    // identifiers, so minification leaves them intact across builds — the most durable anchor
    // available. CSS module class names (`Focusable`) are the next most durable.
    const anchors = {
      focusable: grep('Focusable'),
      gamepadNav: grep('GamepadUI'),
      showModal: grep('showModal'),
      modalRoot: grep('ModalRoot'),
      appContextMenu: grep('#AppProperties_Title'),
      customArtwork: grep('SetCustomArtworkForApp'),
      logoPosition: grep('SetCustomLogoPositionForApp'),
      libraryAssetType: grep('ELibraryAssetType'),
      sliderField: grep('SliderField'),
      // The Properties menu item is what we splice ahead of (S5).
      propertiesMenuItem: grep('#AppDetails_Properties'),
    };

    const counts = {};
    for (const [k, v] of Object.entries(anchors)) counts[k] = v.length;

    // Execute ONLY the narrowest candidate set, to confirm a real exported component is
    // reachable rather than merely mentioned in some module's source.
    let focusableExport = null;
    const focusableCandidates = anchors.focusable.slice(0, 40);
    for (const id of focusableCandidates) {
      try {
        const mod = require(id);
        if (!mod || typeof mod !== 'object') continue;
        for (const key of Object.keys(mod)) {
          const val = mod[key];
          const isComponent =
            typeof val === 'function' ||
            (typeof val === 'object' && val !== null && ('render' in val || '$$typeof' in val));
          if (!isComponent) continue;
          const name = (val && (val.displayName || val.name)) || '';
          const src = typeof val === 'function' ? String(val).slice(0, 400) : '';
          if (/Focusable/.test(name) || /Focusable/.test(src)) {
            focusableExport = { moduleId: id, exportKey: key, name: String(name).slice(0, 60) };
            break;
          }
        }
        if (focusableExport) break;
      } catch {
        // A factory that throws when run in isolation is normal; skip it.
      }
    }

    return {
      hasChunkArray: true,
      captured: true,
      moduleCount: ids.length,
      unreadable,
      anchorCounts: counts,
      // A few ids per anchor, enough to hand-inspect in DevTools.
      anchorSamples: Object.fromEntries(
        Object.entries(anchors).map(([k, v]) => [k, v.slice(0, 5)]),
      ),
      focusableExport,
    };
  });

  // -- S6: what does the CSP in this realm allow? ---------------------------------------
  section('csp', () => {
    const meta = [...document.querySelectorAll('meta[http-equiv]')]
      .filter((m) => /content-security-policy/i.test(m.getAttribute('http-equiv') || ''))
      .map((m) => m.getAttribute('content'));

    const result = { metaPolicies: meta, websocket: 'pending', image: 'pending' };

    // WebSocket to loopback decides the RPC transport. Port 1 is deliberately closed: a
    // *connection refused* proves CSP allowed the attempt, which is what we're testing. A
    // CSP block raises a SecurityError synchronously instead.
    try {
      const ws = new WebSocket('ws://127.0.0.1:1');
      result.websocket = 'allowed-by-csp';
      try { ws.close(); } catch {}
    } catch (e) {
      result.websocket = 'blocked: ' + String((e && e.name) || e);
    }

    // Image loading is async; the caller re-reads these shortly after.
    //
    // An <img> onerror cannot distinguish "CSP blocked it" from "404". So probe BOTH:
    // a fetch() reports a TypeError with a CSP-specific message, and a control request to a
    // host we know resolves separates "network is unreachable from this realm" from
    // "steamgriddb specifically is blocked".
    const startImage = (key, url) => {
      window[key] = 'pending';
      try {
        const img = new Image();
        img.onload = () => { window[key] = 'loaded ' + img.naturalWidth + 'x' + img.naturalHeight; };
        img.onerror = () => { window[key] = 'error (404 or CSP — see the fetch result)'; };
        img.src = url;
      } catch (e) {
        window[key] = 'threw: ' + String(e);
      }
    };

    // A real asset thumbnail, not a favicon that may simply not exist.
    startImage('__sgdbProbeImage', 'https://cdn2.steamgriddb.com/thumb/dc5c768b5dc76a084531934b34601977.jpg');
    // Control: Steam's own CDN, which the client certainly loads from normally.
    startImage('__sgdbProbeControl', 'https://shared.steamstatic.com/store_item_assets/steam/apps/440/header.jpg');
    result.image = 'started';

    // fetch() gives a distinguishable error message when CSP is the cause.
    window.__sgdbProbeFetch = 'pending';
    try {
      fetch('https://cdn2.steamgriddb.com/api/public/health', { method: 'GET', mode: 'no-cors' })
        .then((r) => { window.__sgdbProbeFetch = 'ok type=' + r.type + ' status=' + r.status; })
        .catch((e) => { window.__sgdbProbeFetch = 'rejected: ' + String((e && e.message) || e); });
    } catch (e) {
      window.__sgdbProbeFetch = 'threw: ' + String((e && e.message) || e);
    }

    return result;
  });

  // -- Big Picture state ----------------------------------------------------------------
  section('bigPicture', () => {
    const ui = window.SteamUIStore;
    return {
      // Steam exposes the current UI mode here on recent builds; shape is not guaranteed.
      uiMode: ui && ui.WindowStore ? 'SteamUIStore.WindowStore present' : 'unknown',
      bodyClasses: String(document.body.className).slice(0, 300),
      // In gamepad UI these globals differ; a cheap signal for whether BPM is open.
      gamepadUIRoot: !!document.querySelector('[class*="gamepad"], [class*="Gamepad"]'),
    };
  });

  return out;
})();
