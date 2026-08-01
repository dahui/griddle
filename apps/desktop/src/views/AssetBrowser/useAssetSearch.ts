/**
 * Fetching and paginating one tab's artwork.
 *
 * Lifted out of the browser component because it is the part with the subtle rules: request
 * supersession, the observer that must be re-armed rather than waited on, and the query key that
 * folds the tab and the filters into one value. The component around it is then just layout.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { queryFor, type AssetType, type Filters } from '@griddle/shared';
import { api, asUiError, type Asset, type UiError } from '../../api';

export interface AssetSearch {
  assets: Asset[];
  page: number;
  total: number;
  hasMore: boolean;
  loading: boolean;
  error: UiError | null;
  /** Watched by the infinite-scroll observer. Render it below the grid. */
  sentinel: React.RefObject<HTMLDivElement | null>;
  loadPage: (page: number) => Promise<void>;
  /** True once the stored filters have arrived and this tab actually fetches. */
  ready: boolean;
}

export function useAssetSearch({
  appId,
  assetType,
  filters,
  browsing,
  reloadKey,
}: {
  appId: number;
  assetType: AssetType;
  /** `null` until the stored filters arrive; nothing is fetched before then. */
  filters: Filters | null;
  /** False on the overview tab, which issues no requests at all. */
  browsing: boolean;
  /** Bumped to force a fresh page 0 when the results change without the query changing. */
  reloadKey: number;
}): AssetSearch {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [page, setPage] = useState(0);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<UiError | null>(null);
  const sentinel = useRef<HTMLDivElement | null>(null);

  // `browsing` is part of readiness so the overview tab issues no requests at all, and so
  // returning to a browse tab refetches — which matters, because a reset may have happened in
  // between and the grid would otherwise still show the old state.
  const ready = filters !== null && browsing;

  /**
   * The query parameters, clamped to the current tab.
   *
   * A JSON string rather than an object, and the reason `loadPage` does not depend on `filters`
   * directly: `filters` would be a fresh reference on every render, so using it as a dependency
   * would rebuild `loadPage`, re-fire the reset effect below, and refetch page 0 forever. It
   * also folds the tab in — `queryFor` clamps to `assetType`, so a tab change changes this key.
   */
  const queryKey = useMemo(
    () => JSON.stringify(filters ? queryFor(assetType, filters) : {}),
    [assetType, filters],
  );

  /**
   * Every request carries a generation, and a newer one **supersedes** an older one.
   *
   * This replaces a plain `if (inFlight) return;` guard, which was right for the scroll
   * observer and badly wrong for everything else: when a tab or filter change fired while a
   * request was in flight, the corrective fetch was refused and never retried, so the stale
   * response landed and stuck. That is what made the wrong-filters bug permanent rather than a
   * flicker.
   *
   * `inFlightPage` still exists, but only to stop the observer requesting the same page twice.
   */
  const generation = useRef(0);
  const inFlightPage = useRef<number | null>(null);

  const loadPage = useCallback(
    async (next: number) => {
      const mine = ++generation.current;
      inFlightPage.current = next;
      setLoading(true);
      try {
        const result = await api.searchAssets(
          appId,
          assetType,
          next,
          JSON.parse(queryKey) as Record<string, string>,
        );
        // Superseded: a tab, filter or game change happened while this was in flight. Drop it
        // silently — merging it would mix two different result sets.
        if (generation.current !== mine) return;
        setAssets((prev) => {
          // Deduplicate by id: a page boundary can repeat an item if the underlying list
          // changed between requests, and React would then warn about duplicate keys.
          const seen = new Set(prev.map((a) => a.id));
          return [...prev, ...result.assets.filter((a) => !seen.has(a.id))];
        });
        setTotal(result.total);
        setHasMore(result.has_more);
        setPage(next);
        setError(null);
      } catch (e: unknown) {
        if (generation.current !== mine) return;
        setError(asUiError(e));
        setHasMore(false);
      } finally {
        // Only the newest request owns the shared flags; a superseded one must leave them to
        // whichever request replaced it, or the observer would fire during that one.
        if (generation.current === mine) {
          inFlightPage.current = null;
          setLoading(false);
        }
      }
    },
    [appId, assetType, queryKey],
  );

  // Reset and load the first page whenever the game, tab or filter set changes — `loadPage` is
  // rebuilt for each of those, and for nothing else.
  //
  // `reloadKey` covers the one case that changes the *results* without changing `loadPage`:
  // overriding which SteamGridDB game we pull from. The Steam appid is unchanged, so without it
  // the new game's assets would be appended to the old game's.
  useEffect(() => {
    if (!ready) return;
    setAssets([]);
    setPage(0);
    setTotal(0);
    setHasMore(true);
    setError(null);
    void loadPage(0);
  }, [loadPage, ready, reloadKey]);

  /**
   * Infinite scroll.
   *
   * **`assets.length` and `loading` are in the dependencies on purpose, and removing them
   * strands the list.** An `IntersectionObserver` fires once when it starts observing and then
   * only when the intersection *changes*. That initial callback lands while page 0 is still in
   * flight, so it hits the guard below and does nothing — and `setPage(0)` on page 0 changes
   * nothing, so this effect would not re-run to try again. From then on the only thing that
   * could wake it is the sentinel moving in or out of view, which never happens if the page that
   * arrived was too short to push it past the 400px margin.
   *
   * That is not a corner case: SteamGridDB pages before applying some filters, so a game with a
   * lot of artwork can return a handful of items for page 0 while `total` still promises
   * hundreds — and the browser sits there showing a fraction of them. Re-observing after every
   * settled load replaces "wait for a change" with "ask again", which cannot get stuck.
   */
  useEffect(() => {
    const node = sentinel.current;
    if (!node || !hasMore || error || !ready || loading) return undefined;
    const observer = new IntersectionObserver(
      (entries) => {
        // Only the observer needs this guard: without it, scrolling requests the same next page
        // repeatedly and duplicates every card.
        if (inFlightPage.current !== null) return;
        if (entries.some((e) => e.isIntersecting)) void loadPage(page + 1);
      },
      // Start fetching before the sentinel is actually visible, so scrolling stays smooth.
      { rootMargin: '400px' },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasMore, error, page, loadPage, ready, loading, assets.length]);

  return { assets, page, total, hasMore, loading, error, sentinel, loadPage, ready };
}
