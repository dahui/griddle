/**
 * Browse and apply SteamGridDB artwork for one game.
 *
 * Three files: this one owns the tab, the filters and the apply action; `useAssetSearch` owns
 * fetching and pagination; `tiles.tsx` owns the markup.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ASSET_LABEL,
  ASSET_TYPES,
  defaultFilters,
  fromStored,
  toStored,
  type AssetType,
  type Filters,
  type StoredFilters,
} from '@griddle/shared';
import { api, type GameMatch, type LibraryEntry } from '../../api';
import { Empty, ErrorNote, Spinner, StickyBar, useErrorToast, useToast } from '../../components';
import { SCREEN_DEPTH, useFocusGrid, useScreenActions } from '../../focus';
import { CurrentAssets } from '../CurrentAssets';
import { FilterPanel } from '../FilterPanel';
import { GameSearchModal } from '../GameSearchModal';
import { AssetTab, AssetTile, BackButton, LoadMore } from './tiles';
import { useAssetSearch } from './useAssetSearch';

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
  const [applying, setApplying] = useState<number | null>(null);
  // Mirrors `applying` for the re-entry check in `apply`, which has to read the *current* value
  // rather than the one captured when the handler was created.
  const applyingRef = useRef<number | null>(null);
  const toast = useToast();
  const toastError = useErrorToast();
  // One filter set for every tab. `null` until the stored value arrives — which is also the
  // gate on fetching, because a request built before then would use the wrong filters.
  //
  // The per-tab clamp happens at *query* time, in `queryFor`, never here. Storing a clamped
  // set would throw away a size the moment the user visited a tab that cannot show it; and
  // clamping late is what stops the Wide Capsule tab being queried with the Capsule tab's
  // `600x900`, which returns portrait art rather than an error.
  const [filters, setFilters] = useState<Filters | null>(null);
  const [picking, setPicking] = useState(false);
  const [match, setMatch] = useState<GameMatch | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const { assets, page, total, hasMore, loading, error, sentinel, loadPage, ready } =
    useAssetSearch({ appId: entry.app_id, assetType, filters, browsing, reloadKey });

  // The one grid whose column count changes without the container resizing: `.assets` swaps
  // its `minmax` per asset tab (9.5rem capsules, 22rem heroes). `useFocusGrid` watches child
  // mutations as well as size for exactly this reason.
  const assetGrid = useFocusGrid<HTMLDivElement>('assets');

  // The innermost screen: B leaves the game, and the bumpers own the six asset tabs.
  // `BROWSER_TABS` is `ASSET_TYPES` plus 'current', in the order the bar renders them, so cycling
  // matches what the eye follows rather than some internal ordering.
  const cycleTab = useCallback(
    (step: 1 | -1) =>
      setTab(
        (t) =>
          BROWSER_TABS[
            (BROWSER_TABS.indexOf(t) + step + BROWSER_TABS.length) % BROWSER_TABS.length
          ] ?? DEFAULT_TAB,
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

  async function apply(assetId: number, url: string) {
    // An explicit guard, because the implicit one is gone. Every tile used to carry
    // `disabled={applying !== null}`, which prevented a second apply as a side effect of being
    // unclickable — and also made all of them unfocusable, which a controller cannot survive.
    // Now only the tile in flight is disabled, so the re-entry check has to be stated.
    if (applyingRef.current !== null) return;
    applyingRef.current = assetId;
    setApplying(assetId);
    try {
      const result = await api.applyAsset(entry.app_id, assetType, url);
      // The live path is invisible when it works — the art simply changes in Steam — so
      // "Applied" is the whole message. A restart *being* needed is the only thing worth more.
      toast({
        kind: result.method === 'live' ? 'ok' : 'info',
        message: result.method === 'live' ? 'Applied.' : 'Applied. Restart Steam to see it.',
        action: result.fell_back_because,
      });
    } catch (e: unknown) {
      // A toast, not `setError`. That state drives the *load* failure display, and an apply
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
        <span className="count">{browsing && total > 0 ? `${assets.length} of ${total}` : ''}</span>
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
              onApply={() => void apply(asset.id, asset.url)}
            />
          ))}
        </div>
      )}

      {/* `!ready` matters: nothing is fetched until the stored filters arrive, so without it
          there is a frame showing neither a spinner nor a result. */}
      {browsing && (loading || !ready) && <Spinner label="Loading artwork…" />}
      {browsing && !loading && !hasMore && assets.length === 0 && !error && (
        <Empty>
          SteamGridDB has no {ASSET_LABEL[assetType].toLowerCase()} artwork for this game.
        </Empty>
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
