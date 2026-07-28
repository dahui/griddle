/**
 * Spike probe 11 — **S3: does artwork apply live, with no Steam restart?**
 *
 * This is the difference between matching the Decky plugin and merely matching Steam Art
 * Manager. Every file-based Windows tool needs a restart; the Decky plugin does not, because
 * it calls this API from inside Steam's JS realm.
 *
 * Uses Valve's own convention, read out of module 87498 (probe 10):
 *
 *   SteamClient.Apps.SetCustomArtworkForApp(appid, bareBase64, "png", eAssetType)
 *
 * — the payload is **bare base64** (no `data:` prefix) and the mime is **literally `"png"`**
 * whatever the bytes actually are.
 *
 * ⚠️ **This one writes.** It targets the non-Steam shortcut `4048848997`
 * (EmulationStationDE) and replaces its Capsule art with a magenta test image. The caller
 * backs up `grid/` beforehand and restores afterwards.
 */
(async () => {
  const out = { steps: [] };
  const log = (step, detail) => out.steps.push({ step, ...detail });

  // 60x90 solid magenta PNG — unmistakable against real artwork.
  const TEST_PNG =
    'iVBORw0KGgoAAAANSUhEUgAAADwAAABaCAIAAABrM6JiAAAAXElEQVR42u3OAQkAAAgDsPePYknfQhAG' +
    'C7Bs5p1IS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tL' +
    'S0tLS0tLS18rqc0G5gCazroAAAAASUVORK5CYII=';

  // The unsigned form of the shortcut's signed appid (0xF1548865 / -246118299).
  const APPID = 4048848997;
  const CAPSULE = 0; // ELibraryAssetType.Capsule

  const apps = SteamClient && SteamClient.Apps;
  if (!apps || typeof apps.SetCustomArtworkForApp !== 'function') {
    return { verdict: 'FAIL — SetCustomArtworkForApp unavailable' };
  }

  // Does Steam recognise this appid at all? If the overview is missing, a "successful" call
  // may simply be a no-op against an unknown app.
  log('overview', (() => {
    try {
      const ov = window.appStore && window.appStore.GetAppOverviewByAppID(APPID);
      return ov
        ? {
            found: true,
            name: String(ov.display_name || ov.name || ''),
            isShortcut: typeof ov.BIsShortcut === 'function' ? ov.BIsShortcut() : null,
            appid: ov.appid,
          }
        : { found: false };
    } catch (e) {
      return { error: String(e) };
    }
  })());

  // Apply.
  try {
    const t0 = Date.now();
    const r = await apps.SetCustomArtworkForApp(APPID, TEST_PNG, 'png', CAPSULE);
    log('apply', {
      ms: Date.now() - t0,
      returned: r === undefined ? 'undefined' : JSON.stringify(r).slice(0, 200),
    });
  } catch (e) {
    log('apply', { error: String((e && e.message) || e) });
    return { ...out, verdict: 'FAIL — SetCustomArtworkForApp threw' };
  }

  // Give the client a moment to write the file and invalidate its cache.
  await new Promise((r) => setTimeout(r, 1200));

  // Ask Steam what art it now believes this app has — the in-client view of success, as
  // distinct from what landed on disk (which the caller checks separately).
  log('afterAssetUrl', (() => {
    try {
      const ov = window.appStore && window.appStore.GetAppOverviewByAppID(APPID);
      if (!ov) return { note: 'no overview' };
      const ds = window.appDetailsStore;
      const custom =
        ds && typeof ds.GetCustomVerticalCapsuleURL === 'function'
          ? String(ds.GetCustomVerticalCapsuleURL(ov)).slice(0, 200)
          : null;
      return {
        customVerticalCapsuleURL: custom,
        // A changing cache-buster in the URL is how the client signals it re-read the file.
        hasCacheBuster: custom ? /[?&]/.test(custom) : null,
      };
    } catch (e) {
      return { error: String(e) };
    }
  })());

  out.verdict = 'APPLIED — check the on-disk diff and whether the UI updated without a restart';
  return out;
})();
