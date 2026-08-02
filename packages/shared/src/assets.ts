/**
 * Asset types and their per-type option tables.
 *
 * Note `grid_l` is SteamGridDB's wide capsule, which Steam calls `Header` — the two vocabularies
 * do not line up, and this file is where the frontend's half of that is defined.
 *
 * Steam's own `ELibraryAssetType` ordinals used to be mirrored here too. They are not, now: the
 * frontend never calls Steam, it calls Rust, so `griddle_core::grid::names::AssetType` is the
 * only copy. Two hand-maintained tables of the same measured ordinals is exactly the kind of
 * drift that would send hero art to the capsule slot.
 */

/** SteamGridDB's names for the asset kinds we support. */
export type AssetType = 'grid_p' | 'grid_l' | 'hero' | 'logo' | 'icon';

export const ASSET_TYPES: readonly AssetType[] = ['grid_p', 'grid_l', 'hero', 'logo', 'icon'];

export const ASSET_LABEL: Record<AssetType, string> = {
  grid_p: 'Capsule',
  grid_l: 'Wide Capsule',
  hero: 'Hero',
  logo: 'Logo',
  icon: 'Icon',
};

/**
 * Selectable dimensions per asset type, with the subset that is on by default.
 *
 * **Every value here was probed against the live API on 2026-07-30.** An unrecognised
 * dimension is an HTTP 400, not an empty result, so a wrong entry breaks a whole tab the
 * moment someone ticks it.
 *
 * Two corrections came out of that:
 *
 * - **`icon` takes no dimensions at all.** The endpoint 400s on *every* value — `8x8`,
 *   `16x16`, `32x32`, `64x64`, `128x128`, `256x256`, `512x512`, `1024x1024` were each tried.
 *   The bare-number list that used to be here (`'1024'`, `'768'`, …) was not even the right
 *   *shape*, let alone accepted.
 * - **Values are endpoint-specific.** `heroes?dimensions=600x900` is a 400. The grid sizes and
 *   the hero sizes are not interchangeable, and `griddle-core` enforces that before sending.
 *
 * `512x512` and `1024x1024` *are* valid for grids (9 and 22 assets for Portal 2), but are off
 * by default: they match little and are not the shape Steam renders.
 */
export const DIMENSIONS: Record<AssetType, { all: string[]; default: string[] }> = {
  grid_p: {
    all: ['600x900', '342x482', '660x930', '512x512', '1024x1024'],
    default: ['600x900', '342x482', '660x930'],
  },
  grid_l: {
    all: ['460x215', '920x430', '512x512', '1024x1024'],
    default: ['460x215', '920x430'],
  },
  hero: {
    all: ['1920x620', '3840x1240', '1600x650'],
    default: ['1920x620', '3840x1240', '1600x650'],
  },
  logo: { all: [], default: [] },
  icon: { all: [], default: [] },
};

/** Selectable styles per asset type. SteamGridDB calls `material` "Minimal" in the UI. */
export const STYLES: Record<AssetType, string[]> = {
  grid_p: ['alternate', 'white_logo', 'no_logo', 'blurred', 'material'],
  grid_l: ['alternate', 'white_logo', 'no_logo', 'blurred', 'material'],
  hero: ['alternate', 'blurred', 'material'],
  logo: ['official', 'white', 'black', 'custom'],
  icon: ['official', 'custom'],
};

export const STYLE_LABEL: Record<string, string> = {
  alternate: 'Alternate',
  white_logo: 'White Logo',
  no_logo: 'No Logo',
  blurred: 'Blurred',
  material: 'Minimal',
  official: 'Official',
  white: 'White',
  black: 'Black',
  custom: 'Custom',
};

/** Selectable MIME types per asset type. */
export const MIMES: Record<AssetType, string[]> = {
  grid_p: ['image/png', 'image/jpeg', 'image/webp'],
  grid_l: ['image/png', 'image/jpeg', 'image/webp'],
  hero: ['image/png', 'image/jpeg', 'image/webp'],
  logo: ['image/png', 'image/webp'],
  icon: ['image/png', 'image/vnd.microsoft.icon'],
};

/**
 * Zoom slider bounds. Grid-like types are sized in pixels per card; hero and logo are laid
 * out in columns instead, because their aspect ratios make a pixel width meaningless.
 */
export const ZOOM: Record<AssetType, { min: number; max: number; default: number; unit: 'px' | 'cols' }> = {
  grid_p: { min: 100, max: 200, default: 150, unit: 'px' },
  grid_l: { min: 160, max: 280, default: 220, unit: 'px' },
  icon: { min: 100, max: 200, default: 150, unit: 'px' },
  hero: { min: 2, max: 4, default: 3, unit: 'cols' },
  logo: { min: 2, max: 6, default: 4, unit: 'cols' },
};

/**
 * The path segment SteamGridDB's own site uses for each asset kind.
 *
 * Both capsule types collapse to `grid`, the same way both are served by the `grids` API
 * endpoint and separated only by `dimensions`. There is no `/grid_p/` route.
 */
const ASSET_PAGE_SEGMENT: Record<AssetType, string> = {
  grid_p: 'grid',
  grid_l: 'grid',
  hero: 'hero',
  logo: 'logo',
  icon: 'icon',
};

/**
 * SteamGridDB's page for one asset — where to report it, upvote it, or find more by its author.
 *
 * **The route shapes were measured, not guessed** `[VERIFIED-BOX 2026-08-01]`. Each returned 200
 * with a title naming the game and the author (`grid/1`, `logo/1`, `icon/1`, `hero/100`); an id
 * that does not exist 404s (`hero/1`, `grid/99999999`), as does a segment that is not a route.
 *
 * It has to be probed with a browser `User-Agent`: the *site* is Cloudflare-gated and returns 403
 * to a bare client, unlike the API, which does not. That is why this could not be checked the
 * obvious way, and it is worth knowing before someone re-verifies it and reads the 403 as proof
 * the URL is wrong.
 *
 * `browser::open` refuses anything that is not https on steamgriddb.com or a subdomain, so this
 * is inside the allowlist by construction.
 */
export function assetPageUrl(type: AssetType, id: number): string {
  return `https://www.steamgriddb.com/${ASSET_PAGE_SEGMENT[type]}/${id}`;
}

/**
 * Whether a SteamGridDB preview URL is a **video** rather than an image.
 *
 * Animated artwork is served with a `.webm` *thumbnail* — the full asset is a WebP or an
 * APNG, but the preview is a video. Putting that in an `<img>` renders a broken-image icon,
 * which is exactly what it looks like: missing artwork. On Cyberpunk 2077, **23 of 200**
 * capsules (12%) hit this. `[VERIFIED-BOX 2026-07-30]`
 *
 * **Test the extension, not the mime.** The obvious check — `mime === 'image/webp'` — misses
 * a third of them: 7 of those 23 report `image/png`, because an APNG is animated too and also
 * gets a `.webm` preview. Measured cross-tab on the same 200:
 *
 * | thumb | mime | count |
 * |---|---|---|
 * | `.jpg` | `image/png` | 139 |
 * | `.jpg` | `image/jpeg` | 27 |
 * | `.webm` | `image/webp` | 16 |
 * | `.png` | `image/png` | 11 |
 * | `.webm` | **`image/png`** | **7** |
 */
export function isVideoPreview(url: string | null | undefined): boolean {
  if (!url) return false;
  // Compare the path only: a query string or fragment must not defeat the check, and must not
  // make a `.webm?foo=.jpg` look like an image either.
  const path = url.split(/[?#]/, 1)[0] ?? '';
  return path.toLowerCase().endsWith('.webm');
}

