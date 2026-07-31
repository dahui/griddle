/**
 * S8 — does the Icon asset type do anything for a **real Steam app**?
 *
 * This matters because the Decky plugin handles icons in two completely different ways:
 *
 *   - non-Steam shortcuts → write `<appid>_icon.<ext>` into `grid/` **and** set the `icon`
 *     field in `shortcuts.vdf` (which needs Steam shut down, then a restart)
 *   - real Steam apps     → "poison the cache" by writing
 *     `appcache/librarycache/<appid>_icon.jpg`
 *
 * That second path targets the **legacy flat** librarycache layout. This machine's cache is
 * sha1-keyed (`<appid>/<sha1>.jpg`, and `<appid>/<sha1>/<name>.ext`), so Decky's approach is
 * `[INFERRED]` to be a no-op on current clients.
 *
 * The question here is narrower and more useful: does `SetCustomArtworkForApp(..., Icon)`
 * — Steam's *own* API — work for a real app, and where does it write? If it does, we skip the
 * cache poisoning entirely.
 *
 * ⚠️ **WRITES.** Targets the appid in `window.__SGDB_APPID__`. Snapshot `grid/` and the app's
 * `librarycache` directory first.
 */
(async () => {
  const out = { steps: [] };
  const log = (s, d) => out.steps.push({ step: s, ...d });

  const APPID = Number(window.__SGDB_APPID__);
  // Overridable so the ordinals can be mapped empirically -- decky's typings do
  // not match this build (4 produced the WIDE capsule, not an icon).
  const ICON = Number(window.__SGDB_ASSET_TYPE__ ?? 4);
  const b64 = window.__SGDB_PAYLOAD__;

  if (!APPID) return { verdict: 'FAIL — set window.__SGDB_APPID__' };
  if (typeof b64 !== 'string' || b64.length < 50) {
    return { verdict: 'FAIL — pass --payload <base64-file>' };
  }

  const apps = SteamClient && SteamClient.Apps;
  if (!apps || typeof apps.SetCustomArtworkForApp !== 'function') {
    return { verdict: 'FAIL — apply API unavailable' };
  }

  log('target', (() => {
    try {
      const ov = window.appStore && window.appStore.GetAppOverviewByAppID(APPID);
      return {
        appid: APPID,
        found: !!ov,
        name: ov ? String(ov.display_name || '') : null,
        isShortcut: ov && typeof ov.BIsShortcut === 'function' ? ov.BIsShortcut() : null,
        // If the client exposes an icon hash, a change in it is the in-client success signal.
        iconHash: ov ? String(ov.icon_hash || ov.m_strIconHash || '') : null,
      };
    } catch (e) { return { error: String(e) }; }
  })());

  try {
    const t0 = Date.now();
    await apps.SetCustomArtworkForApp(APPID, b64, 'png', ICON);
    log('apply', { ms: Date.now() - t0, assetType: ICON });
  } catch (e) {
    log('apply', { error: String((e && e.message) || e) });
    return { ...out, verdict: 'THREW — Icon may be unsupported for Steam apps' };
  }

  await new Promise((r) => setTimeout(r, 1500));

  log('afterHash', (() => {
    try {
      const ov = window.appStore && window.appStore.GetAppOverviewByAppID(APPID);
      return { iconHash: ov ? String(ov.icon_hash || ov.m_strIconHash || '') : null };
    } catch (e) { return { error: String(e) }; }
  })());

  out.verdict = 'APPLIED — diff grid/ and librarycache to see what actually changed';
  return out;
})();
