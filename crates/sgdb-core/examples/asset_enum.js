/**
 * De-mangle `ELibraryAssetType`.
 *
 * The ordinals used so far came from decky-frontend-lib's typings
 * (`Capsule 0, Hero 1, Logo 2, Header 3, Icon 4, HeroBlur 5`). Ordinal 0 is confirmed
 * correct — it wrote `<appid>p.png`, the portrait capsule. But passing **4** for a real Steam
 * app produced `<appid>.png`, which is the **wide capsule** filename, not an icon. So at
 * least one ordinal in that table does not match this build.
 *
 * Given five name-based assumptions have already missed on this bundle, the fix is to read
 * the enum out of Steam's own code rather than trust any typing.
 *
 * The members are mangled (`vt.JoK`, `vt.n4o`, …) but module 87498 uses them next to Steam's
 * own asset-name strings (`store_capsule_main`, `library_logo_transparent`). Those strings
 * survive minification, so they are the way in.
 *
 * Read-only.
 */
(() => {
  const out = {};

  let require = null;
  try {
    window.webpackChunksteamui.push([['sgdb_enum_' + Math.random()], {}, (r) => { require = r; }]);
  } catch (e) {
    return { error: 'capture failed: ' + String(e) };
  }
  if (!require || !require.m) return { error: 'no require.m' };

  const src = (id) => { try { return String(require.m[id]); } catch { return ''; } };

  // -- 1. The switch that maps enum members to asset names ---------------------------------
  out.assetNameSwitch = (() => {
    const s = src('87498');
    const i = s.indexOf('store_capsule_main');
    return i === -1 ? null : s.slice(Math.max(0, i - 700), i + 500).replace(/\s+/g, ' ');
  })();

  // -- 2. Find the enum object itself -------------------------------------------------------
  // Look for an object literal whose members are small integers and which is referenced by
  // the same module that calls SetCustomArtworkForApp.
  out.enumCandidates = (() => {
    const found = [];
    for (const id of Object.keys(require.m)) {
      const s = src(id);
      // An enum emitted by TS looks like: X[X.Name=0]="Name" — but names here are mangled,
      // so match the numeric-assignment shape instead.
      const m = s.match(/\w+\[\w+\.(\w+)\s*=\s*(\d)\]\s*=\s*"(\w+)"/g);
      if (m && m.length >= 4) {
        found.push({ moduleId: id, sample: m.slice(0, 8) });
        if (found.length >= 6) break;
      }
    }
    return found;
  })();

  // -- 3. Resolve live values, if the enum is exported ---------------------------------------
  out.liveEnums = (() => {
    const res = [];
    for (const id of Object.keys(require.m)) {
      const s = src(id);
      if (!/LibraryAssetType|library_logo_transparent|store_capsule_main/.test(s)) continue;
      let mod;
      try { mod = require(id); } catch { continue; }
      if (!mod || typeof mod !== 'object') continue;
      for (const k of Object.keys(mod)) {
        let v;
        try { v = mod[k]; } catch { continue; }
        if (!v || typeof v !== 'object') continue;
        let keys;
        try { keys = Object.keys(v); } catch { continue; }
        // An enum has few members, all small integers (plus reverse string mappings).
        const nums = keys.filter((kk) => typeof v[kk] === 'number' && v[kk] >= 0 && v[kk] <= 12);
        if (nums.length >= 4 && nums.length <= 12) {
          res.push({
            moduleId: id,
            exportKey: k,
            members: Object.fromEntries(nums.map((kk) => [kk, v[kk]])),
            reverse: Object.fromEntries(
              keys.filter((kk) => /^\d+$/.test(kk)).slice(0, 12).map((kk) => [kk, String(v[kk])]),
            ),
          });
          if (res.length >= 8) return res;
        }
      }
    }
    return res;
  })();

  return out;
})();
