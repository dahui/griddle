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

export interface LibraryEntry {
  app_id: number;
  name: string;
  kind: 'steam' | 'shortcut';
  app_type: string | null;
  current_art: string | null;
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

export const api = {
  status: () => invoke<Status>('status'),
  setApiKey: (key: string) => invoke<void>('set_api_key', { key }),
  clearApiKey: () => invoke<void>('clear_api_key'),
  library: (assetType: AssetType) => invoke<LibraryEntry[]>('library', { assetType }),
  searchAssets: (appId: number, assetType: AssetType, page: number) =>
    invoke<SearchResult>('search_assets', { appId, assetType, page }),
  applyAsset: (appId: number, assetType: AssetType, url: string) =>
    invoke<Applied>('apply_asset', { appId, assetType, url }),
  clearAsset: (appId: number, assetType: AssetType) =>
    invoke<void>('clear_asset', { appId, assetType }),
  setLiveApply: (enabled: boolean) => invoke<Status>('set_live_apply', { req: { enabled } }),
  removeSentinel: () => invoke<Status>('remove_sentinel'),
  resolveModules: () => invoke<ModuleReport>('resolve_modules'),
};
