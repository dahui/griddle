/** Browse and apply SteamGridDB artwork for one game, with infinite scroll. */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ASSET_LABEL, type AssetType } from '@sgdb/shared';
import { api, asUiError, type Applied, type Asset, type LibraryEntry, type UiError } from '../api';
import { Empty, ErrorNote, Flags, Spinner } from '../components';

export function AssetBrowser({
  entry,
  assetType,
  onBack,
}: {
  entry: LibraryEntry;
  assetType: AssetType;
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

  const sentinel = useRef<HTMLDivElement | null>(null);
  // Guards against the observer firing again while a fetch is already in flight, which would
  // request the same page several times and duplicate every card.
  const inFlight = useRef(false);

  const loadPage = useCallback(
    async (next: number) => {
      if (inFlight.current) return;
      inFlight.current = true;
      setLoading(true);
      try {
        const result = await api.searchAssets(entry.app_id, assetType, next);
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
    [entry.app_id, assetType],
  );

  // Reset and load the first page whenever the game or tab changes.
  useEffect(() => {
    setAssets([]);
    setPage(0);
    setTotal(0);
    setHasMore(true);
    setError(null);
    setApplied(null);
    void loadPage(0);
  }, [loadPage]);

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
