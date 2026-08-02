/**
 * Typed wrappers around the Rust bridge.
 *
 * Every failure from Rust arrives as a {@link UiError} with a machine-readable `kind`, so the
 * UI can branch on *what went wrong* rather than pattern-matching an English sentence. That is
 * the whole reason the Rust side carries a discriminant across the boundary.
 */
import { invoke } from '@tauri-apps/api/core';
// `StoredFilters` is the wire shape of Rust's `FilterState`. Imported rather than redeclared
// here: a second copy of a nine-field struct is a drift waiting to happen, and the field that
// would drift first is `static`, which is a rename on the Rust side.
import type { AssetType, LogoPosition, StoredFilters, ZoomTarget } from '@griddle/shared';

export type ErrorKind =
  | 'no_api_key'
  | 'unauthorized'
  | 'network'
  | 'not_on_steam_grid_db'
  | 'steam_not_found'
  | 'steam_running'
  | 'live_apply_unavailable'
  | 'filesystem'
  | 'unexpected';

export interface UiError {
  kind: ErrorKind;
  message: string;
  /** What the user can do about it. Most environmental failures have one. */
  action: string | null;
}

/**
 * Narrow an unknown rejection to a {@link UiError}.
 *
 * A rejection that is not our shape (a bug in the bridge, a panic) still has to render as
 * *something*, so it degrades to `unexpected` rather than crashing the view.
 */
export function asUiError(e: unknown): UiError {
  if (typeof e === 'object' && e !== null && 'kind' in e && 'message' in e) {
    return e as UiError;
  }
  return { kind: 'unexpected', message: String(e), action: null };
}

export interface Status {
  steam_root: string | null;
  steam_source: string | null;
  account_id: number | null;
  steam_running: boolean;
  has_api_key: boolean;
  /** Whether the CEF debugging flag is in place. Set up at startup; not a user setting. */
  sentinel_present: boolean;
  sentinel_explanation: string;
  app_types_loaded: number | null;
  cache_bytes: number;
  steam_error: string | null;
}

/**
 * Which apps the library list shows.
 *
 * `all` is not "owned". There is no offline ownership list — this is every app Steam holds
 * local config for, which in practice means everything played or configured. It can miss a game
 * you own and never launched.
 */
export type LibraryScope = 'installed' | 'all';

export type LibrarySort = 'name' | 'recently_played' | 'most_played';

export interface LibraryEntry {
  app_id: number;
  name: string;
  /**
   * False when `name` is a stand-in built from the appid.
   *
   * Rare by design: an app Steam has no record of is one the account no longer holds, and those
   * are dropped from the list. This is left true-by-exception for the degraded cases — an
   * unreadable `appinfo.vdf`, or a shortcut with no name of its own.
   */
  named: boolean;
  kind: 'steam' | 'shortcut';
  app_type: string | null;
  /** The user's own art for this slot. */
  current_art: string | null;
  /** Steam's own cached art for this slot, when it is on disk. */
  steam_art: string | null;
  installed: boolean;
  /** Unix seconds. Null when never played — Steam's 1970 sentinel is filtered out in Rust. */
  last_played: number | null;
  playtime_minutes: number | null;
}

export interface Author {
  name: string;
  steam64: string | null;
  avatar: string | null;
}

export interface Asset {
  id: number;
  url: string;
  thumb: string | null;
  /** Zero is legal — icons routinely report 0x0. Never derive an aspect ratio blindly. */
  width: number;
  height: number;
  style: string | null;
  mime: string | null;
  language: string | null;
  notes: string | null;
  nsfw: boolean;
  humor: boolean;
  epilepsy: boolean;
  lock: boolean;
  score: number;
  upvotes: number;
  downvotes: number;
  author: Author;
}

export interface SearchResult {
  assets: Asset[];
  page: number;
  total: number;
  has_more: boolean;
}

export interface Applied {
  method: 'live' | 'file';
  needs_restart: boolean;
  path: string | null;
  replaced: string[];
  fell_back_because: string | null;
}

/** What one artwork slot currently holds. */
export interface AssetSlot {
  asset_type: AssetType;
  label: string;
  /** The user's own artwork, if they have set any. */
  custom_art: string | null;
  /** Steam's own artwork — what a reset falls back to. */
  steam_art: string | null;
  /** Bare filenames a reset would delete, so the UI can name them before it happens. */
  removes: string[];
}

/** Where a custom logo sits, and what a reset would restore. */
export interface LogoPlacement {
  /** Null when this app has never had a position written. */
  position: LogoPosition | null;
  default: LogoPosition;
}

/** What moving the logo actually did. */
export interface LogoMoved {
  method: 'live' | 'file';
  needs_restart: boolean;
  path: string | null;
  fell_back_because: string | null;
}

/** What a reset actually did. */
export interface Cleared {
  method: 'live' | 'file';
  needs_restart: boolean;
  removed: string[];
  fell_back_because: string | null;
}

/** What a full reset would remove. Counted without deleting anything. */
export interface ResetPlan {
  games: number;
  files: number;
}

/** What a full reset actually did. */
export interface ResetAll {
  games: number;
  files_removed: number;
  method: 'live' | 'file';
  needs_restart: boolean;
  fell_back_because: string | null;
  /** Games whose files could not be removed. Empty on a clean run. */
  failed: string[];
}

/**
 * The live-apply self-test.
 *
 * Two fields on purpose. This replaced a module-map report that graded eleven structural finders
 * against Steam's bundle — machinery the Big Picture deliverable needed and nothing else did.
 * `SetCustomArtworkForApp` is bound by the CEF host rather than shipped in Steam's JS, so a
 * `typeof` check is the entire compatibility surface that remains.
 */
export interface LiveApplyCheck {
  /** Steam's build stamp. Shown so a bug report can name the build; never acted on. */
  clstamp: string | null;
  can_apply: boolean;
}

export interface Prefs {
  library_scope: LibraryScope;
  library_sort: LibrarySort;
  /**
   * The content filters, shared by every asset type.
   *
   * `null` when the user has never changed them; the gap is filled with `defaultFilters()`
   * rather than by Rust, so the defaults have exactly one implementation.
   */
  filters: StoredFilters | null;
  /** Tile width per grid, in rem. Includes the library list and the Current overview. */
  zoom: Partial<Record<ZoomTarget, number>>;
  /** Steam appid → the SteamGridDB game to pull from, for when the automatic match is wrong. */
  game_overrides: Record<number, { id: number; name: string | null }>;
}

/** A SteamGridDB game. `id` is SteamGridDB's own id, not a Steam appid. */
export interface GameMatch {
  id: number;
  name: string;
  verified: boolean;
  types: string[];
}

export const api = {
  status: () => invoke<Status>('status'),
  /**
   * Open a link in the default browser.
   *
   * The webview cannot do this itself — Tauri ignores `target="_blank"` — and the backend only
   * accepts an allowlisted https URL, so this is not a general "launch anything".
   */
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  setApiKey: (key: string) => invoke<void>('set_api_key', { key }),
  clearApiKey: () => invoke<void>('clear_api_key'),
  /**
   * `sort` is passed rather than read from the stored settings, because persisting the choice
   * and reloading the list are separate round trips and the read would race the write.
   */
  library: (assetType: AssetType, scope: LibraryScope, sort: LibrarySort) =>
    invoke<LibraryEntry[]>('library', { assetType, scope, sort }),
  /** `filters` is the output of `filtersToQuery()`; omit it to use the tab's defaults. */
  searchAssets: (
    appId: number,
    assetType: AssetType,
    page: number,
    filters?: Record<string, string>,
  ) => invoke<SearchResult>('search_assets', { appId, assetType, page, filters }),
  applyAsset: (appId: number, assetType: AssetType, url: string) =>
    invoke<Applied>('apply_asset', { appId, assetType, url }),
  assetStatus: (appId: number) => invoke<AssetSlot[]>('asset_status', { appId }),
  /** Read from the file, not from Steam, so the positioner works with Steam closed. */
  logoPlacement: (appId: number) => invoke<LogoPlacement>('logo_placement', { appId }),
  /** Live if Steam is running, and the file is written either way. */
  setLogoPlacement: (appId: number, position: LogoPosition) =>
    invoke<LogoMoved>('set_logo_placement', { appId, position }),
  clearAsset: (appId: number, assetType: AssetType) =>
    invoke<Cleared>('clear_asset', { appId, assetType }),
  /** Read-only: counts what a full reset would delete, so the confirmation can quote it. */
  resetAllPlan: () => invoke<ResetPlan>('reset_all_plan'),
  resetAllArt: () => invoke<ResetAll>('reset_all_art'),
  prefs: () => invoke<Prefs>('prefs'),
  setLibraryView: (scope: LibraryScope, sort: LibrarySort) =>
    invoke<Prefs>('set_library_view', { scope, sort }),
  /** One filter set, shared by every asset type. */
  setFilters: (filters: StoredFilters) => invoke<Prefs>('set_filters', { filters }),
  resetFilters: () => invoke<Prefs>('reset_filters'),
  /**
   * Tile width for one grid of artwork, in rem. Bounds are `ZOOM`'s, not Rust's.
   *
   * `ZoomTarget`, not `AssetType`: the library list and the Current overview are resizable too
   * and are not asset types. Rust validates the name against the same seven.
   */
  setZoom: (target: ZoomTarget, value: number) =>
    invoke<Prefs>('set_zoom', { assetType: target, value }),
  /**
   * `null` clears the override and returns to the automatic Steam-appid match.
   *
   * `name` is stored alongside purely so the UI can name the override later; there is no by-id
   * lookup, and this project does not ship an endpoint it has not probed against the live API.
   */
  setGameOverride: (appId: number, sgdbId: number | null, name: string | null) =>
    invoke<Prefs>('set_game_override', { appId, sgdbId, name }),
  searchGames: (term: string) => invoke<GameMatch[]>('search_games', { term }),
  currentGameMatch: (appId: number) => invoke<GameMatch | null>('current_game_match', { appId }),
  /** Connects to Steam and reports whether artwork applies without a restart. */
  liveApplyCheck: () => invoke<LiveApplyCheck>('live_apply_check'),
};
