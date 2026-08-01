/** Browse and apply SteamGridDB artwork for one game, with filters and infinite scroll. */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ASSET_LABEL,
  ASSET_TYPES,
  defaultFilters,
  fromStored,
  isVideoPreview,
  queryFor,
  toStored,
  type AssetType,
  type Filters,
  type StoredFilters,
} from '@griddle/shared';
import {
  api,
  asUiError,
  type Asset,
  type GameMatch,
  type LibraryEntry,
  type UiError,
} from '../api';
import {
  Empty,
  ErrorNote,
  Flags,
  Spinner,
  StickyBar,
  useErrorToast,
  useToast,
} from '../components';
import { SCREEN_DEPTH, useFocusGrid, useFocusGridItem, useFocusItem, useScreenActions } from '../focus';
import { CurrentAssets } from './CurrentAssets';
import { FilterPanel } from './FilterPanel';
import { GameSearchModal } from './GameSearchModal';

/**
 * The tab every game opens on.
 *
 * Portrait capsule, because it is the one the library grid shows and so the one the user was
 * looking at when they clicked. `Settings.tabs.default_tab` exists to make this a preference
 * later; until then a constant is honest about what it is.
 */
const DEFAULT_TAB: AssetType = 'grid_p';

/** The five browsing tabs, plus the overview of what is currently applied. */
type BrowserTab = AssetType | 'current';

/** In render order, so the bumpers cycle through them the way the tab bar reads. */
const BROWSER_TABS: BrowserTab[] = [...ASSET_TYPES, 'current'];

export function AssetBrowser({ entry, onBack }: { entry: LibraryEntry; onBack: () => void }) {
  // Held here, not in `App`, so it resets to the Capsule tab for every game. This component is
  // unmounted whenever the library list is showing, which makes that structural — there is no
  // "reset the tab" call anyone can forget when a new game is opened.
  const [tab, setTab] = useState<BrowserTab>(DEFAULT_TAB);
  const browsing = tab !== 'current';
  // A concrete type is still needed to key the fetch state; nothing is fetched while the
  // overview is showing, so the placeholder is never actually queried.
  const assetType: AssetType = browsing ? tab : DEFAULT_TAB;
  const [assets, setAssets] = useState<Asset[]>([]);
  const [page, setPage] = useState(0);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<UiError | null>(null);
  const [applying, setApplying] = useState<number | null>(null);
  // Mirrors `applying` for the re-entry check in `apply`, which has to read the *current* value
  // rather than the one captured when the handler was created.
  const applyingRef = useRef<number | null>(null);
  const toast = useToast();
  const toastError = useErrorToast();
  // One filter set for every tab. `null` until the stored value arrives — which is also the
  // gate on fetching, because a request built before then would use the wrong filters.
  //
  // 🔴 The per-tab clamp happens at *query* time, in `queryFor`, never here. Storing a clamped
  // set would throw away a size the moment the user visited a tab that cannot show it; and
  // clamping late is what stops the Wide Capsule tab being queried with the Capsule tab's
  // `600x900`, which returns portrait art rather than an error.
  const [filters, setFilters] = useState<Filters | null>(null);
  const [picking, setPicking] = useState(false);
  const [match, setMatch] = useState<GameMatch | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const sentinel = useRef<HTMLDivElement | null>(null);
  // 🔴 The one grid whose column count changes without the container resizing: `.assets` swaps
  // its `minmax` per asset tab (9.5rem capsules, 22rem heroes). `useFocusGrid` watches child
  // mutations as well as size for exactly this reason.
  const assetGrid = useFocusGrid<HTMLDivElement>('assets');

  // The innermost screen: B leaves the game, and the bumpers own the six asset tabs. `BROWSER_TABS`
  // is `ASSET_TYPES` plus 'current', in the order the bar renders them, so cycling matches what the
  // eye follows rather than some internal ordering.
  const cycleTab = useCallback(
    (step: 1 | -1) =>
      setTab(
        (t) =>
          BROWSER_TABS[(BROWSER_TABS.indexOf(t) + step + BROWSER_TABS.length) % BROWSER_TABS.length] ??
          DEFAULT_TAB,
      ),
    [],
  );
  useScreenActions(SCREEN_DEPTH.game, {
    onBack,
    onTabPrev: () => cycleTab(-1),
    onTabNext: () => cycleTab(1),
  });

  // Read once. A tab change needs no round trip, so there is no window in which the filters in
  // hand belong to something other than what is about to be queried.
  useEffect(() => {
    let cancelled = false;
    api
      .prefs()
      .then((p) => {
        if (!cancelled) setFilters(fromStored(p.filters));
      })
      // Unreadable settings must not block browsing — fall through to the defaults.
      .catch(() => {
        if (!cancelled) setFilters(defaultFilters());
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // `browsing` is part of readiness so the overview tab issues no requests at all, and so
  // returning to a browse tab refetches — which matters, because a reset may have happened in
  // between and the grid would otherwise still show the old state.
  const ready = filters !== null && browsing;

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
   * 🔴 This replaces a plain `if (inFlight) return;` guard, which was right for the scroll
   * observer and badly wrong for everything else: when a tab or filter change fired while a
   * request was in flight, the corrective fetch was refused and never retried, so the stale
   * response landed and stuck. That is what made the wrong-filters bug above permanent rather
   * than a flicker.
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
          entry.app_id,
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
    [entry.app_id, assetType, queryKey],
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
   * 🔴 **`assets.length` and `loading` are in the dependencies on purpose, and removing them
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

  /**
   * Apply a filter change locally and persist it.
   *
   * Local state leads so the grid refetches immediately rather than waiting on the write, and
   * failing to *persist* a filter choice must not surface as a browsing error.
   */
  function applyFilters(next: Filters, persist: () => Promise<{ filters: StoredFilters | null }>) {
    setFilters(next);
    void persist().catch(() => undefined);
  }

  function changeFilters(next: Filters) {
    applyFilters(next, () => api.setFilters(toStored(next)));
  }

  function resetFilters() {
    applyFilters(defaultFilters(), () => api.resetFilters());
  }

  async function apply(asset: Asset) {
    // 🔴 An explicit guard, because the implicit one is gone. Every tile used to carry
    // `disabled={applying !== null}`, which prevented a second apply as a side effect of being
    // unclickable — and also made all of them unfocusable, which a controller cannot survive.
    // Now only the tile in flight is disabled, so the re-entry check has to be stated.
    if (applyingRef.current !== null) return;
    applyingRef.current = asset.id;
    setApplying(asset.id);
    try {
      const result = await api.applyAsset(entry.app_id, assetType, asset.url);
      // The live path is invisible when it works — the art simply changes in Steam — so
      // "Applied" is the whole message. A restart *being* needed is the only thing worth more.
      toast({
        kind: result.method === 'live' ? 'ok' : 'info',
        message: result.method === 'live' ? 'Applied.' : 'Applied. Restart Steam to see it.',
        action: result.fell_back_because,
      });
    } catch (e: unknown) {
      // 🔴 A toast, not `setError`. That state drives the *load* failure display, and an apply
      // that fails leaves the grid perfectly usable — putting it there replaced a working list
      // with an error box, or sat below one that was about something else entirely.
      toastError(e);
    } finally {
      applyingRef.current = null;
      setApplying(null);
    }
  }

  return (
    <>
      {/* Sticky, because this is the only way back and infinite scroll can put it a very long
          way above you. The game name and count come along for free, which is what makes the
          pinned bar read as a header rather than a stray button. */}
      <StickyBar className="toolbar">
        <BackButton onClick={onBack} />
        <strong className="browsing">{entry.name}</strong>
        {/* Loaded *and* total, not just total. A count that only ever showed the total is what
            let "the browser is quietly showing 12 of 400" go unnoticed. The tab above already
            says which kind of artwork these are. */}
        <span className="count">
          {browsing && total > 0 ? `${assets.length} of ${total}` : ''}
        </span>
      </StickyBar>

      {/* The asset slots belong to a single game, which is why this bar lives here and not in
          the app-level nav — on the library list it was a control with nothing to control. */}
      <nav className="tab-group asset-tabs">
        {ASSET_TYPES.map((t, i) => (
          <AssetTab key={t} col={i} active={tab === t} onClick={() => setTab(t)}>
            {ASSET_LABEL[t]}
          </AssetTab>
        ))}
        {/* Last, so the first tab is still the one every game opens on. */}
        <AssetTab
          col={ASSET_TYPES.length}
          active={tab === 'current'}
          onClick={() => setTab('current')}
        >
          Current
        </AssetTab>
      </nav>

      {!browsing && <CurrentAssets entry={entry} />}

      {browsing && filters && (
        <FilterPanel
          assetType={assetType}
          filters={filters}
          onChange={changeFilters}
          onReset={resetFilters}
          onPickGame={() => setPicking(true)}
          gameLabel={match?.name ?? null}
        />
      )}

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

      {browsing && error && assets.length === 0 && (
        <ErrorNote error={error} onRetry={() => void loadPage(0)} />
      )}

      {browsing && (
        <div className={`assets assets-${assetType}`} ref={assetGrid} aria-busy={applying !== null}>
          {assets.map((asset, index) => (
            <AssetTile
              key={asset.id}
              index={index}
              asset={asset}
              label={ASSET_LABEL[assetType].toLowerCase()}
              applying={applying === asset.id}
              anyApplying={applying !== null}
              onApply={() => void apply(asset)}
            />
          ))}
        </div>
      )}

      {/* `!ready` matters: nothing is fetched until the stored filters arrive, so without it
          there is a frame showing neither a spinner nor a result. */}
      {browsing && (loading || !ready) && <Spinner label="Loading artwork…" />}
      {browsing && !loading && !hasMore && assets.length === 0 && !error && (
        <Empty>SteamGridDB has no {ASSET_LABEL[assetType].toLowerCase()} artwork for this game.</Empty>
      )}
      {browsing && error && assets.length > 0 && <ErrorNote error={error} />}

      {/* An explicit way to load the rest. Infinite scroll depends on the viewport geometry
          working out, and when it does not there is otherwise nothing the user can do — the
          remaining artwork is simply unreachable. Shown only when there is genuinely more. */}
      {browsing && !loading && hasMore && !error && assets.length > 0 && (
        <div className="load-more">
          <LoadMore onClick={() => void loadPage(page + 1)} />
        </div>
      )}

      <div ref={sentinel} className="sentinel" />
    </>
  );
}

/**
 * One asset's preview.
 *
 * 🔴 Animated artwork's thumbnail is a **`.webm` video**, not an image, and an `<img>` renders
 * it as a broken-image icon — indistinguishable from missing artwork. 12% of Cyberpunk 2077's
 * capsules are affected. See `isVideoPreview` for why the extension, not the mime, is the test.
 *
 * The `<video>` needs `muted` for `autoPlay` to be allowed at all, and the CSP already carries
 * `media-src https://cdn2.steamgriddb.com` — someone anticipated this and the renderer did not
 * catch up.
 */
function AssetPreview({ asset }: { asset: Asset }) {
  const src = asset.thumb ?? asset.url;
  if (isVideoPreview(src)) {
    return (
      <video
        src={src}
        autoPlay
        loop
        muted
        playsInline
        // `metadata` rather than `auto`: a page of these is tens of megabytes, and the first
        // frame is enough to show what the artwork is.
        preload="metadata"
      />
    );
  }
  return <img src={src} alt="" loading="lazy" />;
}

/** The way out. Its own section, above the tabs, and the first thing the pad reaches going up. */
function BackButton({ onClick }: { onClick: () => void }) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('back', 0, 0);
  return (
    <button
      ref={ref}
      type="button"
      className={`ghost${focused ? ' focused' : ''}`}
      onClick={onClick}
    >
      ← Library
    </button>
  );
}

function AssetTab({
  col,
  active,
  onClick,
  children,
}: {
  col: number;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('asset-tabs', 0, col);
  return (
    <button
      ref={ref}
      type="button"
      className={`tab${active ? ' active' : ''}${focused ? ' focused' : ''}`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function LoadMore({ onClick }: { onClick: () => void }) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('load-more', 0, 0);
  return (
    <button
      ref={ref}
      type="button"
      className={`ghost${focused ? ' focused' : ''}`}
      onClick={onClick}
    >
      Load more
    </button>
  );
}

/**
 * One piece of candidate artwork.
 *
 * 🔴 `disabled` is now only on the tile actually being applied. It used to be on **every** tile
 * whenever any apply was in flight, which is invisible with a mouse but destroys keyboard and pad
 * navigation outright: a disabled button cannot hold focus, so the grid would empty out
 * mid-action and focus would be flung elsewhere. `aria-busy` on the container carries the "work
 * in progress" meaning instead, and a second click is guarded by the apply path itself.
 */
function AssetTile({
  index,
  asset,
  label,
  applying,
  anyApplying,
  onApply,
}: {
  index: number;
  asset: Asset;
  label: string;
  applying: boolean;
  anyApplying: boolean;
  onApply: () => void;
}) {
  const { ref, focused } = useFocusGridItem<HTMLButtonElement>('assets', index);
  return (
    <figure className="asset">
      <button
        ref={ref}
        type="button"
        className={`asset-button${focused ? ' focused' : ''}`}
        disabled={applying}
        aria-disabled={anyApplying}
        onClick={onApply}
        title={`Apply this ${label}`}
      >
        <AssetPreview asset={asset} />
        {applying && <span className="applying">Applying…</span>}
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
  );
}

