/**
 * What every artwork slot for this game is currently set to.
 *
 * The overview the five browsing tabs cannot give: which slots the user has customised, which
 * are still Steam's own, and which have nothing at all.
 *
 * Right-clicking a slot resets it. That deletion is **named before it happens** — the menu lists
 * the files it will remove — because this project deletes nothing from a user's Steam directory
 * without saying so first.
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import { useCallback, useEffect, useState } from 'react';
import { steamCdnUrl, type AssetType } from '@sgdb/shared';
import { api, asUiError, type AssetSlot, type Cleared, type LibraryEntry, type UiError } from '../api';
import { ArtImage, ContextMenu, ErrorNote, Spinner } from '../components';

type Menu = { x: number; y: number; slot: AssetSlot };

export function CurrentAssets({ entry }: { entry: LibraryEntry }) {
  const [slots, setSlots] = useState<AssetSlot[] | null>(null);
  // Two error slots, deliberately. A load failure means there is nothing to show; a *reset*
  // failure must not take the view with it — replacing the grid with an error box loses the
  // context the user needs, and hides which slot failed.
  const [loadError, setLoadError] = useState<UiError | null>(null);
  const [resetError, setResetError] = useState<UiError | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [busy, setBusy] = useState<AssetType | null>(null);
  const [cleared, setCleared] = useState<{ slot: string; result: Cleared } | null>(null);

  const load = useCallback(() => {
    let cancelled = false;
    api
      .assetStatus(entry.app_id)
      .then((s) => {
        if (!cancelled) {
          setSlots(s);
          setLoadError(null);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) setLoadError(asUiError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [entry.app_id]);

  useEffect(load, [load]);

  async function reset(slot: AssetSlot) {
    setMenu(null);
    setBusy(slot.asset_type);
    setResetError(null);
    setCleared(null);
    try {
      const result = await api.clearAsset(entry.app_id, slot.asset_type);
      setCleared({ slot: slot.label, result });
      load();
    } catch (e: unknown) {
      setResetError(asUiError(e));
    } finally {
      setBusy(null);
    }
  }

  if (loadError) return <ErrorNote error={loadError} onRetry={load} />;
  if (!slots) return <Spinner label="Reading current artwork…" />;

  return (
    <>
      {resetError && <ErrorNote error={resetError} />}
      {cleared && <ClearedNote slot={cleared.slot} result={cleared.result} />}

      <p className="hint">Right-click any artwork to reset it to Steam&rsquo;s own.</p>

      <ul className="slots">
        {slots.map((slot) => (
          <li key={slot.asset_type} className="slot">
            {/* Not a button: this is a display of what is applied, and left-clicking it used to
                jump to that browsing tab — which reads as the view navigating away by itself
                when all you did was look at something. Right-click is the only action. */}
            <div
              className={`slot-art slot-art-${slot.asset_type}`}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY, slot });
              }}
              title={`${slot.label} — right-click to reset`}
            >
              <ArtImage
                sources={sourcesFor(entry, slot)}
                alt=""
                fallback={<span className="art-none">No artwork</span>}
              />
              {busy === slot.asset_type && <span className="applying">Resetting…</span>}
            </div>
            <span className="slot-name">{slot.label}</span>
            <span className={`slot-state ${slot.custom_art ? 'slot-custom' : ''}`}>
              {state(slot)}
            </span>
          </li>
        ))}
      </ul>

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} onClose={() => setMenu(null)}>
          <div className="menu-title">{menu.slot.label}</div>
          {menu.slot.removes.length > 0 ? (
            <button type="button" className="menu-item" onClick={() => void reset(menu.slot)}>
              Reset to Steam&rsquo;s artwork
              {/* Naming the files is the point: nothing is deleted from the user's Steam
                  directory without the UI saying which files first. */}
              <span className="menu-note">Deletes {menu.slot.removes.join(', ')}</span>
            </button>
          ) : (
            <div className="menu-item menu-disabled">
              Nothing to reset
              <span className="menu-note">This slot has no custom artwork.</span>
            </div>
          )}
        </ContextMenu>
      )}
    </>
  );
}

/**
 * The artwork ladder for one slot, best first — the same one the library list walks.
 *
 * Custom art, then Steam's local cache, then Steam's CDN, then a placeholder.
 */
function sourcesFor(entry: LibraryEntry, slot: AssetSlot): string[] {
  return [
    slot.custom_art && convertFileSrc(slot.custom_art),
    slot.steam_art && convertFileSrc(slot.steam_art),
    entry.kind === 'steam' ? steamCdnUrl(entry.app_id, slot.asset_type) : null,
  ].filter((s): s is string => Boolean(s));
}

function state(slot: AssetSlot): string {
  if (slot.custom_art) return 'Custom';
  if (slot.steam_art) return "Steam's own";
  return 'Not set';
}

function ClearedNote({ slot, result }: { slot: string; result: Cleared }) {
  if (result.removed.length === 0) {
    return (
      <div className="note note-info">
        <p className="note-message">{slot} had no custom artwork to remove.</p>
      </div>
    );
  }
  if (result.method === 'live') {
    return (
      <div className="note note-ok">
        <p className="note-message">
          {slot} reset to Steam&rsquo;s artwork. Removed {result.removed.join(', ')} — no restart
          needed.
        </p>
      </div>
    );
  }
  return (
    <div className="note note-info">
      <p className="note-message">
        {slot} reset. Removed {result.removed.join(', ')}.
        {result.needs_restart && ' Restart Steam to see it.'}
      </p>
      {result.fell_back_because && <p className="note-action">{result.fell_back_because}</p>}
    </div>
  );
}
