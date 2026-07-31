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
import type { AssetType, StoredFilters } from '@griddle/shared';

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
 * 🔴 `all` is not "owned". There is no offline ownership list — this is every app Steam holds
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
  /** 🔴 Zero is legal — icons routinely report 0x0. Never derive an aspect ratio blindly. */
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

/** What a reset actually did. */
export interface Cleared {
  method: 'live' | 'file';
  needs_restart: boolean;
  removed: string[];
  fell_back_because: string | null;
}

export interface ModuleReport {
  clstamp: string;
  total_modules: number;
  resolved: number;
  outcomes: [string, string][];
  features: [string, boolean, string][];
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
  zoom: Partial<Record<AssetType, number>>;
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
  clearAsset: (appId: number, assetType: AssetType) =>
    invoke<Cleared>('clear_asset', { appId, assetType }),
  prefs: () => invoke<Prefs>('prefs'),
  setLibraryView: (scope: LibraryScope, sort: LibrarySort) =>
    invoke<Prefs>('set_library_view', { scope, sort }),
  /** One filter set, shared by every asset type. */
  setFilters: (filters: StoredFilters) => invoke<Prefs>('set_filters', { filters }),
  resetFilters: () => invoke<Prefs>('reset_filters'),
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
  resolveModules: () => invoke<ModuleReport>('resolve_modules'),
};
