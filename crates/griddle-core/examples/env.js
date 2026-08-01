/**
 * M1 spike probe. Evaluated inside Steam's SharedJSContext via CDP Runtime.evaluate.
 *
 * Answers S1 (are we really in Steam's realm), S6 (what does the CSP allow), and the feature
 * detection that gates live apply.
 *
 * 🔵 The S2 section — capturing `webpackChunksteamui` and grepping every module's source for
 * gamepad-focusable components — went with the Big Picture deliverable. Its findings, including
 * why source-reading beats Decky's execute-every-factory approach, are in CLAUDE.md.
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

  return out;
})();
