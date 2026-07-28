/**
 * S4 — does an animated asset survive being labelled `"png"`?
 *
 * SteamGridDB serves animated grids as **WebP** (and some as APNG). Steam's own code always
 * passes the literal string `"png"` to `SetCustomArtworkForApp` regardless of the bytes — see
 * module 87498 — so animated WebP ends up in a file called `<appid>p.png` and Chromium sniffs
 * the content. The Decky plugin relies on this. This checks it actually holds.
 *
 * The asset is fetched **in-page** rather than passed in as base64, which mirrors Steam's own
 * flow exactly (fetch → blob → FileReader → strip the data-URL prefix) and avoids shipping an
 * 18 MB literal through CDP. Probe `env` already proved the CSP permits cdn2.steamgriddb.com.
 *
 * ⚠️ **WRITES.** Replaces the Capsule art of shortcut 4048848997. Back up `grid/` first.
 *
 * Pass `--apng` to test the APNG variant instead of the animated WebP.
 */
(async () => {
  const out = { steps: [] };
  const log = (s, d) => out.steps.push({ step: s, ...d });

  const APPID = 4048848997;
  const CAPSULE = 0;

  // Both verified animated before use: WebP has the VP8X animation flag and 300 ANMF frames;
  // the PNG carries acTL and 73 fcTL frames.
  const WEBP = 'https://cdn2.steamgriddb.com/grid/2cffdc4195ce6adf0a57062e4318662e.webp';
  const APNG = 'https://cdn2.steamgriddb.com/grid/58d8e2b3e0c6cef74a997f1b4b5497c7.png';
  const url = window.__SGDB_USE_APNG__ ? APNG : WEBP;

  const apps = SteamClient && SteamClient.Apps;
  if (!apps || typeof apps.SetCustomArtworkForApp !== 'function') {
    return { verdict: 'FAIL — apply API unavailable' };
  }

  try {
    // 🔴 The bytes MUST come from outside this realm.
    //
    // A normal `fetch()` to cdn2.steamgriddb.com from SharedJSContext fails with
    // "Failed to fetch" — CORS. Only `mode:'no-cors'` succeeds, and its response is opaque,
    // so the body cannot be read. SGDB images can be **displayed** here (`<img src>` works)
    // but never **read**. This is why decky-steamgriddb ships a Python `download_as_base64`
    // backend, and why our SGDB client lives in Rust.
    //
    // The harness injects the payload with `--payload <file>`.
    const b64 = window.__SGDB_PAYLOAD__;
    if (typeof b64 !== 'string' || b64.length < 100) {
      // Prove the CORS claim rather than just asserting it.
      let corsError = null;
      try {
        await fetch(url).then((r) => r.blob());
        corsError = 'unexpectedly succeeded';
      } catch (e) {
        corsError = String((e && e.message) || e);
      }
      return {
        ...out,
        verdict: 'no payload — pass --payload <base64-file>',
        inPageFetch: corsError,
      };
    }
    log('payload', { base64Length: b64.length, source: 'injected by harness (Rust-side download)' });

    const t2 = Date.now();
    await apps.SetCustomArtworkForApp(APPID, b64, 'png', CAPSULE);
    log('apply', { ms: Date.now() - t2, note: 'mime argument was the literal "png"' });

    await new Promise((r) => setTimeout(r, 1500));

    // What does the client think the art is now?
    log('after', (() => {
      try {
        const ov = window.appStore && window.appStore.GetAppOverviewByAppID(APPID);
        return { overviewFound: !!ov, name: ov ? String(ov.display_name || '') : null };
      } catch (e) { return { error: String(e) }; }
    })());

    out.verdict = 'APPLIED — check the bytes on disk, then look at the library for motion';
    return out;
  } catch (e) {
    log('error', { message: String((e && e.message) || e).slice(0, 300) });
    return { ...out, verdict: 'ERROR' };
  }
})();
