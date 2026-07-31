/** Browse and apply SteamGridDB artwork for one game, with filters and infinite scroll. */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ASSET_LABEL,
  ASSET_TYPES,
  defaultFilters,
  filtersToQuery,
  fromStored,
  toStored,
  type AssetType,
  type Filters,
} from '@sgdb/shared';
import {
  api,
  asUiError,
  type Applied,
  type Asset,
  type GameMatch,
  type LibraryEntry,
  type UiError,
} from '../api';
import { Empty, ErrorNote, Flags, Spinner } from '../components';
import { FilterPanel } from './FilterPanel';
import { GameSearchModal } from './GameSearchModal';

export function AssetBrowser({
  entry,
  assetType,
  onAssetType,
  onBack,
}: {
  entry: LibraryEntry;
  assetType: AssetType;
  onAssetType: (type: AssetType) => void;
  onBack: () => void;
}) {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [page, setPage] = useState(0);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<UiError | null>(null);
  const [applying, setApplying] = useState<number | null>(null);
  const [applied, setApplied] = useState<Applied | null>(null);
  const [filters, setFilters] = useState<Filters>(() => defaultFilters(assetType));
  const [picking, setPicking] = useState(false);
  const [match, setMatch] = useState<GameMatch | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const sentinel = useRef<HTMLDivElement | null>(null);
  // Guards against the observer firing again while a fetch is already in flight, which would
  // request the same page several times and duplicate every card.
  const inFlight = useRef(false);

  // Load this tab's stored filters. A tab the user has never customised has nothing stored, and
  // `fromStored` fills in the defaults — which live in one place, in TypeScript.
  useEffect(() => {
    let cancelled = false;
    api
      .prefs()
      .then((p) => {
        if (!cancelled) setFilters(fromStored(assetType, p.filters[assetType]));
      })
      .catch(() => {
        if (!cancelled) setFilters(defaultFilters(assetType));
      });
    return () => {
      cancelled = true;
    };
  }, [assetType]);

  // Which SteamGridDB game we are pulling from, for the "Wrong game?" button's label.
  useEffect(() => {
    let cancelled = false;
    setMatch(null);
    api
      .currentGameMatch(entry.app_id)
      .then((m) => {
        if (!cancelled) setMatch(m);
      })
      // Purely informational; a failure here must not disturb browsing.
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [entry.app_id]);

  /**
   * 🔴 A stable key for the filter set, and the reason `loadPage` does not depend on `filters`
   * directly: `filters` is a fresh object on every render, so using it as a dependency would
   * rebuild `loadPage`, re-fire the reset effect below, and refetch page 0 forever.
   */
  const queryKey = useMemo(() => JSON.stringify(filtersToQuery(filters)), [filters]);

  const loadPage = useCallback(
    async (next: number) => {
      if (inFlight.current) return;
      inFlight.current = true;
      setLoading(true);
      try {
        const result = await api.searchAssets(
          entry.app_id,
          assetType,
          next,
          JSON.parse(queryKey) as Record<string, string>,
        );
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
        setError(asUiError(e));
        setHasMore(false);
      } finally {
        setLoading(false);
        inFlight.current = false;
      }
    },
    [entry.app_id, assetType, queryKey],
  );

  // Reset and load the first page whenever the game, tab or filter set changes — `loadPage` is
  // rebuilt for each of those, and for nothing else.
  //
  // `reloadKey` covers the one case that changes the *results* without changing `loadPage`:
  // overriding which SteamGridDB game we pull from. The Steam appid is unchanged, so without it
  // the new game's assets would be appended to the old game's.
  useEffect(() => {
    setAssets([]);
    setPage(0);
    setTotal(0);
    setHasMore(true);
    setError(null);
    setApplied(null);
    void loadPage(0);
  }, [loadPage, reloadKey]);

  useEffect(() => {
    const node = sentinel.current;
    if (!node || !hasMore || error) return undefined;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) void loadPage(page + 1);
      },
      // Start fetching before the sentinel is actually visible, so scrolling stays smooth.
      { rootMargin: '400px' },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasMore, error, page, loadPage]);

  function changeFilters(next: Filters) {
    setFilters(next);
    // Fire and forget: the grid already refetches from the state above, and failing to *persist*
    // a filter choice should not surface as a browsing error.
    void api.setFilters(assetType, toStored(next)).catch(() => undefined);
  }

  function resetFilters() {
    setFilters(defaultFilters(assetType));
    void api.resetFilters(assetType).catch(() => undefined);
  }

  async function apply(asset: Asset) {
    setApplying(asset.id);
    setApplied(null);
    try {
      setApplied(await api.applyAsset(entry.app_id, assetType, asset.url));
    } catch (e: unknown) {
      setError(asUiError(e));
    } finally {
      setApplying(null);
    }
  }

  return (
    <>
      <div className="toolbar">
        <button type="button" className="ghost" onClick={onBack}>
          ← Library
        </button>
        <strong className="browsing">{entry.name}</strong>
        <span className="count">
          {total > 0 ? `${total} ${ASSET_LABEL[assetType].toLowerCase()} options` : ''}
        </span>
      </div>

      {/* The asset slots belong to a single game, which is why this bar lives here and not in
          the app-level nav — on the library list it was a control with nothing to control. */}
      <nav className="tab-group asset-tabs">
        {ASSET_TYPES.map((t) => (
          <button
            type="button"
            key={t}
            className={assetType === t ? 'tab active' : 'tab'}
            onClick={() => onAssetType(t)}
          >
            {ASSET_LABEL[t]}
          </button>
        ))}
      </nav>

      <FilterPanel
        assetType={assetType}
        filters={filters}
        onChange={changeFilters}
        onReset={resetFilters}
        onPickGame={() => setPicking(true)}
        gameLabel={match?.name ?? null}
      />

      {picking && (
        <GameSearchModal
          appId={entry.app_id}
          gameName={entry.name}
          current={match}
          onPicked={(game) => {
            setMatch(game);
            setPicking(false);
            // A different game is an entirely different result set, so clear the grid rather
            // than appending page 0 of the new game to the old game's assets.
            setReloadKey((k) => k + 1);
          }}
          onClose={() => setPicking(false)}
        />
      )}

      {applied && <AppliedNote applied={applied} />}
      {error && assets.length === 0 && <ErrorNote error={error} onRetry={() => void loadPage(0)} />}

      <div className={`assets assets-${assetType}`}>
        {assets.map((asset) => (
          <figure key={asset.id} className="asset">
            <button
              type="button"
              className="asset-button"
              disabled={applying !== null}
              onClick={() => void apply(asset)}
              title={`Apply this ${ASSET_LABEL[assetType].toLowerCase()}`}
            >
              <img src={asset.thumb ?? asset.url} alt="" loading="lazy" />
              {applying === asset.id && <span className="applying">Applying…</span>}
            </button>
            <figcaption>
              <span className="author">{asset.author.name || 'unknown'}</span>
              {/* 0x0 is legal for icons, so only show real dimensions. */}
              {asset.width > 0 && asset.height > 0 && (
                <span className="dims">
                  {asset.width}×{asset.height}
                </span>
              )}
              <Flags asset={asset} />
            </figcaption>
          </figure>
        ))}
      </div>

      {loading && <Spinner label="Loading artwork…" />}
      {!loading && !hasMore && assets.length === 0 && !error && (
        <Empty>SteamGridDB has no {ASSET_LABEL[assetType].toLowerCase()} artwork for this game.</Empty>
      )}
      {error && assets.length > 0 && <ErrorNote error={error} />}
      <div ref={sentinel} className="sentinel" />
    </>
  );
}

/**
 * What the apply actually did.
 *
 * The live path is invisible when it works — the art simply changes in Steam — so the only
 * thing worth saying is when a *restart* is needed, and why the slower path was taken.
 */
function AppliedNote({ applied }: { applied: Applied }) {
  if (applied.method === 'live') {
    return (
      <div className="note note-ok">
        <p className="note-message">Applied. Steam updated straight away — no restart needed.</p>
      </div>
    );
  }
  return (
    <div className="note note-info">
      <p className="note-message">Artwork written to disk. Restart Steam to see it.</p>
      {applied.fell_back_because && <p className="note-action">{applied.fell_back_because}</p>}
      {applied.replaced.length > 0 && (
        <p className="note-action">Replaced {applied.replaced.length} existing file(s).</p>
      )}
    </div>
  );
}
