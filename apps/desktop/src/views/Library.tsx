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
          <button
            type="button"
            className={scope === 'installed' ? 'tab active' : 'tab'}
            onClick={() => view('installed', sort)}
          >
            Installed
          </button>
          <button
            type="button"
            className={scope === 'all' ? 'tab active' : 'tab'}
            onClick={() => view('all', sort)}
            title="Everything Steam has a record of on this PC — not everything you own."
          >
            All games
          </button>
        </div>

        <input
          type="search"
          className="search"
          placeholder="Filter games…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />

        <label className="sort">
          Sort
          <select value={sort} onChange={(e) => view(scope, e.target.value as LibrarySort)}>
            {(Object.keys(SORT_LABEL) as LibrarySort[]).map((s) => (
              <option key={s} value={s}>
                {SORT_LABEL[s]}
              </option>
            ))}
          </select>
        </label>

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
        <ul className="library">
          {shown.map((entry) => (
            <li key={`${entry.kind}-${entry.app_id}`}>
              <button type="button" className="game" onClick={() => onPick(entry)}>
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
          ))}
        </ul>
      )}
    </>
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
