/**
 * Steam's own default artwork, from its public CDN.
 *
 * The last rung before a placeholder. Local `librarycache` covers only a third of apps for the
 * portrait capsule (714 of 2248 on the development box), and a not-installed game often has no
 * local art at all — so without this, the "All games" scope would be mostly empty tiles.
 *
 * These are plain public images: no API key, no Steam Web API, no authentication of any kind.
 * Both hosts are already in the app's CSP `img-src`.
 */
import type { AssetType } from './assets';

/**
 * 🔴 **The CDN basenames are NOT the on-disk basenames.** Measured `[VERIFIED-BOX 2026-07-30]`.
 *
 * The disk name for a slot varies per app — 1030300's capsule is stored as
 * `<sha1>/library_capsule.jpg` — but the CDN serves every app's capsule as
 * `library_600x900.jpg`. Reusing the disk name is the obvious mistake and it 404s:
 *
 * | URL                             | 620 | 1030300 |
 * |---------------------------------|-----|---------|
 * | `library_600x900.jpg`           | 200 | **200** |
 * | `header.jpg`                    | 200 | 200     |
 * | `library_hero.jpg`              | 200 | 200     |
 * | `logo.png`                      | 200 | 200     |
 * | `library_capsule.jpg`           | **404** | **404** |
 * | `library_header.jpg`            | —   | **404** |
 *
 * The two 404 rows are recorded on purpose: they are what stops someone "fixing" this table by
 * copying the names out of `librarycache`.
 */
export const STEAM_CDN_BASENAME: Record<AssetType, string | null> = {
  grid_p: 'library_600x900.jpg',
  grid_l: 'header.jpg',
  hero: 'library_hero.jpg',
  logo: 'logo.png',
  // Icons live on a different host under a different path, and need the app's icon hash from
  // `appinfo.vdf` rather than a fixed name. See {@link steamIconUrl}.
  icon: null,
};

export const STEAM_CDN_BASE = 'https://shared.steamstatic.com/store_item_assets/steam/apps';

/** Where the small icon lives — a different host *and* path from every other asset. */
export const STEAM_ICON_CDN_BASE =
  'https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps';

/**
 * Steam's default art for a real Steam app, or `null` when there is no fixed name for the slot.
 *
 * Returns a URL rather than checking it: an unknown appid 404s, and a non-Steam shortcut appid
 * 404s too, both of which the caller's `onError` ladder already handles. Probing first would
 * double every request to save nothing.
 */
export function steamCdnUrl(appId: number, type: AssetType): string | null {
  const name = STEAM_CDN_BASENAME[type];
  return name ? `${STEAM_CDN_BASE}/${appId}/${name}` : null;
}

/** The small icon, which needs the `common/icon` sha1 from `appinfo.vdf`. */
export function steamIconUrl(appId: number, iconSha1: string): string {
  return `${STEAM_ICON_CDN_BASE}/${appId}/${iconSha1}.jpg`;
}
