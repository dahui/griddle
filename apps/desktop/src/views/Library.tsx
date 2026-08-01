/**
 * The game list.
 *
 * Two things here are worth knowing before changing them:
 *
 * - **The list always shows the portrait capsule.** It used to follow the app-level asset tab,
 *   but those tabs are a per-game control and the list was the one place they did nothing.
 * - **Artwork is a ladder, not a field.** Custom art, then Steam's local cache, then Steam's
 *   CDN, then a text placeholder. Only a third of apps have a locally cached capsule, so
 *   without the CDN rung most tiles would be blank — especially under the "All games" scope,
 *   where most entries are not installed and have no local art at all.
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import { useEffect, useMemo, useState } from 'react';
import { steamCdnUrl, type AssetType } from '@griddle/shared';
import {
  api,
  asUiError,
  type LibraryEntry,
  type LibraryScope,
  type LibrarySort,
  type UiError,
} from '../api';
import { ArtImage, Empty, ErrorNote, Spinner } from '../components';
import { SCREEN_DEPTH, useFocusGrid, useFocusGridItem, useFocusItem, useScreenActions } from '../focus';

const LIST_ASSET: AssetType = 'grid_p';

const SORT_LABEL: Record<LibrarySort, string> = {
  name: 'Name',
  recently_played: 'Recently played',
  most_played: 'Most played',
};

export function Library({ onPick }: { onPick: (entry: LibraryEntry) => void }) {
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [error, setError] = useState<UiError | null>(null);
  const [filter, setFilter] = useState('');
  const [reloadKey, setReloadKey] = useState(0);
  // Null until the stored preferences arrive. Rendering the list before then would load the
  // wrong scope and then reload, which reads as a flicker on every launch.
  const [scope, setScope] = useState<LibraryScope | null>(null);
  const [sort, setSort] = useState<LibrarySort>('name');
  // Unconditional, above the early returns below: `useFocusGrid` is a hook, and the list has two
  // states (error, loading) that return before the grid renders.
  const grid = useFocusGrid<HTMLUListElement>('library');

  useEffect(() => {
    api
      .prefs()
      .then((p) => {
        setScope(p.library_scope);
        setSort(p.library_sort);
      })
      // A settings file we cannot read must not block the library; fall back to the defaults.
      .catch(() => setScope('installed'));
  }, []);

  useEffect(() => {
    if (!scope) return undefined;
    let cancelled = false;
    setEntries(null);
    setError(null);
    api
      .library(LIST_ASSET, scope, sort)
      .then((list) => {
        // Switching scope while a load is in flight would otherwise let the older, larger
        // response land last and show apps the user just filtered out.
        if (!cancelled) setEntries(list);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(asUiError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [scope, sort, reloadKey]);

  function view(nextScope: LibraryScope, nextSort: LibrarySort) {
    setScope(nextScope);
    setSort(nextSort);
    // Fire and forget: the list already re-reads from the state above, and a failure to
    // *persist* a view preference should not surface as a library error.
    void api.setLibraryView(nextScope, nextSort).catch(() => undefined);
  }

  // The library's own "tabs" are the two scopes, so the bumpers toggle them here rather than
  // falling through to the Library/Settings switch. No `onBack`: the list is the root screen and
  // there is nowhere further back to go.
  useScreenActions(SCREEN_DEPTH.library, {
    onTabPrev: () => scope && view(scope === 'all' ? 'installed' : 'all', sort),
    onTabNext: () => scope && view(scope === 'installed' ? 'all' : 'installed', sort),
  });

  const shown = useMemo(() => {
    if (!entries) return [];
    const needle = filter.trim().toLowerCase();
    if (!needle) return entries;
    return entries.filter((e) => e.name.toLowerCase().includes(needle));
  }, [entries, filter]);

  if (error) return <ErrorNote error={error} onRetry={() => setReloadKey((k) => k + 1)} />;
  if (!scope || !entries) return <Spinner label="Loading your library…" />;

  return (
    <>
      <div className="toolbar">
        <div className="tab-group">
          <ScopeTab col={0} active={scope === 'installed'} onClick={() => view('installed', sort)}>
            Installed
          </ScopeTab>
          <ScopeTab
            col={1}
            active={scope === 'all'}
            onClick={() => view('all', sort)}
            title="Everything Steam has a record of on this PC — not everything you own."
          >
            All games
          </ScopeTab>
        </div>

        <FilterBox value={filter} onChange={setFilter} />
        <SortOptions value={sort} onChange={(s) => view(scope, s)} firstCol={3} />

        <span className="count">
          {shown.length === entries.length
            ? `${entries.length} games`
            : `${shown.length} of ${entries.length}`}
        </span>
      </div>

      {shown.length === 0 ? (
        <Empty>
          {entries.length === 0 ? 'No games found.' : `Nothing matches “${filter}”.`}
        </Empty>
      ) : (
        <ul className="library" ref={grid}>
          {shown.map((entry, index) => (
            <GameTile
              key={`${entry.kind}-${entry.app_id}`}
              index={index}
              entry={entry}
              onPick={() => onPick(entry)}
            />
          ))}
        </ul>
      )}
    </>
  );
}

/**
 * The library toolbar is one focus row: two scope tabs, the filter box, then the sort control.
 * Columns are assigned here rather than derived, because the row mixes control types and their
 * DOM order is the only thing that makes it a row at all.
 */
function ScopeTab({
  col,
  active,
  onClick,
  title,
  children,
}: {
  col: number;
  active: boolean;
  onClick: () => void;
  title?: string;
  children: React.ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('toolbar', 0, col);
  return (
    <button
      ref={ref}
      type="button"
      className={`tab${active ? ' active' : ''}${focused ? ' focused' : ''}`}
      onClick={onClick}
      title={title}
    >
      {children}
    </button>
  );
}

function FilterBox({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('toolbar', 0, 2);
  return (
    <input
      ref={ref}
      type="search"
      className={`search${focused ? ' focused' : ''}`}
      placeholder="Filter games…"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/**
 * The sort choice, as three buttons rather than a dropdown.
 *
 * It was a native `<select>`, and a controller **cannot open one**: the popup is an OS widget
 * drawn outside the page, so it receives none of the input this app synthesises. The control was
 * reachable and focusable, which made it worse than an obviously missing one — the cursor landed
 * on it, A did nothing, and there was no way to tell that from a bug.
 *
 * Three ordinary buttons need no new mechanism, are reachable by exactly the same path as
 * everything else, and match the scope tabs sitting beside them in this same toolbar row. It
 * only works because there are three options; a longer list would need a real listbox.
 */
function SortOptions({
  value,
  onChange,
  firstCol,
}: {
  value: LibrarySort;
  onChange: (s: LibrarySort) => void;
  /** Where this group starts in the toolbar row, so the columns stay contiguous. */
  firstCol: number;
}) {
  return (
    <div className="sort" role="group" aria-label="Sort by">
      <span className="sort-label">Sort</span>
      <div className="tab-group">
        {(Object.keys(SORT_LABEL) as LibrarySort[]).map((s, i) => (
          <SortOption
            key={s}
            col={firstCol + i}
            active={value === s}
            onClick={() => onChange(s)}
          >
            {SORT_LABEL[s]}
          </SortOption>
        ))}
      </div>
    </div>
  );
}

function SortOption({
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
  const { ref, focused } = useFocusItem<HTMLButtonElement>('toolbar', 0, col);
  return (
    <button
      ref={ref}
      type="button"
      className={`tab tab-small${active ? ' active' : ''}${focused ? ' focused' : ''}`}
      // The pressed state, not just a colour: a screen reader has no other way to know which of
      // three visually-similar buttons is the current sort.
      aria-pressed={active}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** One game. Split out so it can register itself; a hook cannot live in the parent's `map`. */
function GameTile({
  index,
  entry,
  onPick,
}: {
  index: number;
  entry: LibraryEntry;
  onPick: () => void;
}) {
  const { ref, focused } = useFocusGridItem<HTMLButtonElement>('library', index);
  return (
    <li>
      <button
        ref={ref}
        type="button"
        className={`game${focused ? ' focused' : ''}`}
        onClick={onPick}
      >
        <span className="art">
          <ArtImage
            sources={artSources(entry)}
            alt=""
            fallback={<span className="art-none">No artwork</span>}
          />
        </span>
        <span className="game-name">{entry.name}</span>
        <span className="game-meta">{meta(entry)}</span>
      </button>
    </li>
  );
}

/**
 * The artwork ladder for one entry, best first.
 *
 * `convertFileSrc` routes local paths through Tauri's `asset:` protocol, which is scoped at
 * startup to the account's `grid/` and to Steam's `librarycache/`. A path outside those scopes
 * fails to load rather than erroring loudly — which simply advances the ladder, so a
 * scope-grant failure degrades to the CDN instead of to a broken image.
 */
function artSources(entry: LibraryEntry): string[] {
  return [
    entry.current_art && convertFileSrc(entry.current_art),
    entry.steam_art && convertFileSrc(entry.steam_art),
    // Shortcut appids are not Steam appids, so the CDN would 404 on every one of them.
    entry.kind === 'steam' ? steamCdnUrl(entry.app_id, LIST_ASSET) : null,
  ].filter((s): s is string => Boolean(s));
}

function meta(entry: LibraryEntry): string {
  if (entry.kind === 'shortcut') return 'Non-Steam';
  // Only reachable when `appinfo.vdf` could not be read — an app Steam has no record of is
  // dropped from the list rather than shown. Says why the row has no name instead of leaving
  // something that reads as artwork which failed to load.
  if (!entry.named) return 'Steam has no details for this app';
  const kind = entry.app_type ?? 'Game';
  return entry.installed ? kind : `${kind} · not installed`;
}
