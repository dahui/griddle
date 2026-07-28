/**
 * Asset types and their per-type option tables.
 *
 * The numeric values are Steam's own `ELibraryAssetType` ordinals, passed as the fourth
 * argument to `SteamClient.Apps.SetCustomArtworkForApp`. `HeroBlur` (5) exists in Steam but
 * neither the Decky plugin nor this app edits it.
 *
 * Note `grid_l` maps to `Header` (3), not to something named "wide" — Steam's naming and
 * SteamGridDB's do not line up, and this table is the single place that reconciles them.
 */
export const ELibraryAssetType = {
  Capsule: 0,
  Hero: 1,
  Logo: 2,
  Header: 3,
  Icon: 4,
  HeroBlur: 5,
} as const;

/** SteamGridDB's names for the asset kinds we support. */
export type AssetType = 'grid_p' | 'grid_l' | 'hero' | 'logo' | 'icon';

export const ASSET_TYPES: readonly AssetType[] = ['grid_p', 'grid_l', 'hero', 'logo', 'icon'];

/** SteamGridDB asset kind -> Steam's `ELibraryAssetType` ordinal. */
export const STEAM_ASSET_TYPE: Record<AssetType, number> = {
  grid_p: ELibraryAssetType.Capsule,
  grid_l: ELibraryAssetType.Header,
  hero: ELibraryAssetType.Hero,
  logo: ELibraryAssetType.Logo,
  icon: ELibraryAssetType.Icon,
};

/** The SteamGridDB API path segment for each asset kind. */
export const SGDB_ENDPOINT: Record<AssetType, string> = {
  grid_p: 'grids',
  grid_l: 'grids',
  hero: 'heroes',
  logo: 'logos',
  icon: 'icons',
};

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
 * `logo` deliberately has none — SteamGridDB does not offer a dimension filter for logos.
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
  icon: {
    all: [
      '1024', '768', '512', '310', '256', '194', '192', '180', '160', '152', '150', '144',
      '128', '120', '114', '100', '96', '90', '80', '76', '72', '64', '60', '57', '56', '54',
      '48', '40', '35', '32', '28', '24', '20', '16', '14', '10', '8',
    ],
    default: [],
  },
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
 * A 1x1 fully transparent PNG, used by the "Use Invisible Asset" action to blank an asset
 * without leaving Steam's default art showing through.
 *
 * Not offered for icons — a blank icon reads as a broken install rather than a deliberate one.
 */
export const INVISIBLE_PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVQYV2NgYAAAAAMAAWgmWQ0AAAAASUVORK5CYII=';
