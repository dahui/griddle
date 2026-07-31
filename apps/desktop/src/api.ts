/**
 * Typed wrappers around the Rust bridge.
 *
 * Every failure from Rust arrives as a {@link UiError} with a machine-readable `kind`, so the
 * UI can branch on *what went wrong* rather than pattern-matching an English sentence. That is
 * the whole reason the Rust side carries a discriminant across the boundary.
 */
import { invoke } from '@tauri-apps/api/core';
import type { AssetType } from '@sgdb/shared';

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
  live_apply_enabled: boolean;
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

export interface ModuleReport {
  clstamp: string;
  total_modules: number;
  resolved: number;
  outcomes: [string, string][];
  features: [string, boolean, string][];
}

/** The content filters for one asset type, as stored. Mirrors Rust's `FilterState`. */
export interface FilterState {
  untagged: boolean;
  adult: boolean;
  humor: boolean;
  epilepsy: boolean;
  styles: string[];
  dimensions: string[];
  mimes: string[];
  animated: boolean;
  /** `static` on the wire; Rust calls the field `statik` because it is a keyword there. */
  static: boolean;
}

export interface Prefs {
  library_scope: LibraryScope;
  library_sort: LibrarySort;
  /**
   * Keyed by asset type (`grid_p`, …). **Sparse** — only types the user has customised are
   * present, and the gaps are filled with `defaultFilters(type)` rather than by Rust, so the
   * defaults have exactly one implementation.
   */
  filters: Partial<Record<AssetType, FilterState>>;
  zoom: Partial<Record<AssetType, number>>;
  /** Steam appid → SteamGridDB game id, for when the automatic match is wrong. */
  game_overrides: Record<number, number>;
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
  setApiKey: (key: string) => invoke<void>('set_api_key', { key }),
  clearApiKey: () => invoke<void>('clear_api_key'),
  library: (assetType: AssetType, scope: LibraryScope) =>
    invoke<LibraryEntry[]>('library', { assetType, scope }),
  /** `filters` is the output of `filtersToQuery()`; omit it to use the tab's defaults. */
  searchAssets: (
    appId: number,
    assetType: AssetType,
    page: number,
    filters?: Record<string, string>,
  ) => invoke<SearchResult>('search_assets', { appId, assetType, page, filters }),
  applyAsset: (appId: number, assetType: AssetType, url: string) =>
    invoke<Applied>('apply_asset', { appId, assetType, url }),
  clearAsset: (appId: number, assetType: AssetType) =>
    invoke<void>('clear_asset', { appId, assetType }),
  prefs: () => invoke<Prefs>('prefs'),
  setLibraryView: (scope: LibraryScope, sort: LibrarySort) =>
    invoke<Prefs>('set_library_view', { scope, sort }),
  setFilters: (assetType: AssetType, filters: FilterState) =>
    invoke<Prefs>('set_filters', { assetType, filters }),
  resetFilters: (assetType: AssetType) => invoke<Prefs>('reset_filters', { assetType }),
  /** `null` clears the override and returns to the automatic Steam-appid match. */
  setGameOverride: (appId: number, sgdbId: number | null) =>
    invoke<Prefs>('set_game_override', { appId, sgdbId }),
  searchGames: (term: string) => invoke<GameMatch[]>('search_games', { term }),
  currentGameMatch: (appId: number) => invoke<GameMatch | null>('current_game_match', { appId }),
  setLiveApply: (enabled: boolean) => invoke<Status>('set_live_apply', { req: { enabled } }),
  removeSentinel: () => invoke<Status>('remove_sentinel'),
  resolveModules: () => invoke<ModuleReport>('resolve_modules'),
};
