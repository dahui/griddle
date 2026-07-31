/** The game list: installed Steam apps plus non-Steam shortcuts, showing existing custom art. */
import { convertFileSrc } from '@tauri-apps/api/core';
import { useEffect, useMemo, useState } from 'react';
import type { AssetType } from '@sgdb/shared';
import { api, asUiError, type LibraryEntry, type UiError } from '../api';
import { Empty, ErrorNote, Spinner } from '../components';

export function Library({
  assetType,
  onPick,
}: {
  assetType: AssetType;
  onPick: (entry: LibraryEntry) => void;
}) {
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [error, setError] = useState<UiError | null>(null);
  const [filter, setFilter] = useState('');
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setEntries(null);
    setError(null);
    api
      .library(assetType)
      .then((list) => {
        // The asset type can change while a load is in flight; without this guard the older
        // response can land last and show art for the wrong tab.
        if (!cancelled) setEntries(list);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(asUiError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [assetType, reloadKey]);

  const shown = useMemo(() => {
    if (!entries) return [];
    const needle = filter.trim().toLowerCase();
    if (!needle) return entries;
    return entries.filter((e) => e.name.toLowerCase().includes(needle));
  }, [entries, filter]);

  if (error) return <ErrorNote error={error} onRetry={() => setReloadKey((k) => k + 1)} />;
  if (!entries) return <Spinner label="Reading your Steam library…" />;

  return (
    <>
      <div className="toolbar">
        <input
          type="search"
          className="search"
          placeholder="Filter games…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <span className="count">
          {shown.length === entries.length
            ? `${entries.length} games`
            : `${shown.length} of ${entries.length}`}
        </span>
      </div>

      {shown.length === 0 ? (
        <Empty>
          {entries.length === 0
            ? 'No installed games or shortcuts were found.'
            : `Nothing matches “${filter}”.`}
        </Empty>
      ) : (
        <ul className="library">
          {shown.map((entry) => (
            <li key={`${entry.kind}-${entry.app_id}`}>
              <button type="button" className="game" onClick={() => onPick(entry)}>
                <span className="art">
                  {entry.current_art ? (
                    // `convertFileSrc` routes through Tauri's asset: protocol, which is scoped
                    // at startup to exactly the account's grid/ directory.
                    <img src={convertFileSrc(entry.current_art)} alt="" loading="lazy" />
                  ) : (
                    <span className="art-none">No custom art</span>
                  )}
                </span>
                <span className="game-name">{entry.name}</span>
                <span className="game-meta">
                  {entry.kind === 'shortcut' ? 'Non-Steam' : (entry.app_type ?? 'Game')}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
