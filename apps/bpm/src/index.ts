/**
 * The Big Picture injection bundle.
 *
 * Evaluated inside Steam's `SharedJSContext` — the same JS realm that renders the desktop
 * library *and* Big Picture Mode. See `apps/bpm/vite.config.ts` for why this must build to a
 * single self-contained IIFE.
 *
 * # The idempotency contract
 *
 * The first thing this bundle does is call `unload()` on any previous copy of itself. Every
 * patch registers an undo closure, and `unload()` runs them last-in-first-out.
 *
 * This is not tidiness — it is what makes reconnects, dev hot-reload, and Steam restarts all
 * the *same* code path. Without it, re-injection stacks duplicate context-menu items and
 * duplicate patches, and the result looks like it works right up until it doesn't.
 *
 * M6 fills this in. For now it is the minimum that proves injection round-trips.
 */

import { ASSET_TYPES } from '@griddle/shared';

declare global {
  interface Window {
    __SGDB_CLIENT__?: SgdbClient;
    webpackChunksteamui?: unknown[];
  }
}

interface SgdbClient {
  version: string;
  /** Steam's build stamp, if we can read it. Keys the module map cache. */
  clstamp: string | null;
  /** Runs every registered undo closure, LIFO. Safe to call more than once. */
  unload(): void;
  /** Registers an undo closure. Returns it, so callers can compose. */
  onUnload(fn: () => void): () => void;
  /** Diagnostics the Rust side reads back over the RPC bridge. */
  probe(): Record<string, unknown>;
}

// Tear down any previous injection before installing this one.
window.__SGDB_CLIENT__?.unload?.();

const undo: Array<() => void> = [];

function readClstamp(): string | null {
  // Valve declares `var CLSTAMP="10840511";` on line 1 of steamui/library.js, so it is a
  // global in this realm. [VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]
  const value = (globalThis as Record<string, unknown>).CLSTAMP;
  return typeof value === 'string' ? value : null;
}

const client: SgdbClient = {
  version: '0.0.0',
  clstamp: readClstamp(),

  unload() {
    while (undo.length > 0) {
      const fn = undo.pop();
      try {
        fn?.();
      } catch (e) {
        // One failing teardown must not strand the rest — a half-unloaded injection is worse
        // than a noisy one.
        console.error('[sgdb] teardown step failed:', e);
      }
    }
    if (window.__SGDB_CLIENT__ === client) {
      delete window.__SGDB_CLIENT__;
    }
  },

  onUnload(fn) {
    undo.push(fn);
    return fn;
  },

  probe() {
    return {
      version: client.version,
      clstamp: client.clstamp,
      assetTypes: ASSET_TYPES.length,
      // The module-discovery hook. Its absence means S2's approach is unavailable on this
      // build and we are in the degraded, plain-DOM path.
      hasWebpackChunk: Array.isArray(window.webpackChunksteamui),
      // Feature detection for the live-apply path, checked before we rely on it.
      hasSetCustomArtwork:
        typeof (globalThis as Record<string, any>).SteamClient?.Apps?.SetCustomArtworkForApp ===
        'function',
      hasSetLogoPosition:
        typeof (globalThis as Record<string, any>).SteamClient?.Apps
          ?.SetCustomLogoPositionForApp === 'function',
    };
  },
};

window.__SGDB_CLIENT__ = client;

console.info('[sgdb] injected', client.probe());
