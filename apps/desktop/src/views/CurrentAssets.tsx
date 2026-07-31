/**
 * What every artwork slot for this game is currently set to.
 *
 * The overview the five browsing tabs cannot give: which slots the user has customised, which
 * are still Steam's own, and which have nothing at all.
 *
 * Clicking a slot shows it larger; right-clicking resets it. That deletion is **named before it
 * happens** — the menu lists the files it will remove — because this project deletes nothing from
 * a user's Steam directory without saying so first.
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import { useCallback, useEffect, useState } from 'react';
import { steamCdnUrl, type AssetType } from '@griddle/shared';
import { api, asUiError, type AssetSlot, type LibraryEntry, type UiError } from '../api';
import {
  ArtImage,
  ContextMenu,
  ErrorNote,
  Spinner,
  useErrorToast,
  useToast,
} from '../components';

type Menu = { x: number; y: number; slot: AssetSlot };

export function CurrentAssets({ entry }: { entry: LibraryEntry }) {
  const [slots, setSlots] = useState<AssetSlot[] | null>(null);
  // Only the *load* failure is held here, and only because it is the view: with no slots there
  // is nothing to render but the error. A reset failure leaves the grid perfectly readable, so
  // it goes to a toast rather than displacing what the user is looking at.
  const [loadError, setLoadError] = useState<UiError | null>(null);
  const toast = useToast();
  const toastError = useErrorToast();
  const [menu, setMenu] = useState<Menu | null>(null);
  // The slot *type*, not the slot. A reset reloads `slots` into fresh objects, and holding one of
  // the old ones here would leave the preview showing artwork that is no longer there.
  const [preview, setPreview] = useState<AssetType | null>(null);
  const [busy, setBusy] = useState<AssetType | null>(null);

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
    try {
      const result = await api.clearAsset(entry.app_id, slot.asset_type);
      // Nothing removed means nothing changed, so there is nothing worth saying. The menu does
      // not offer the action in that state anyway.
      if (result.removed.length > 0) {
        toast({
          kind: result.method === 'live' ? 'ok' : 'info',
          message: `${slot.label} reset.${result.needs_restart ? ' Restart Steam to see it.' : ''}`,
          action: result.fell_back_because,
        });
      }
      load();
    } catch (e: unknown) {
      toastError(e);
    } finally {
      setBusy(null);
    }
  }

  if (loadError) return <ErrorNote error={loadError} onRetry={load} />;
  if (!slots) return <Spinner label="Reading current artwork…" />;

  // Resolved from the live list every render, so the preview can never outlive the slot it shows.
  const previewSlot = slots.find((s) => s.asset_type === preview) ?? null;

  return (
    <>
      <p className="hint">Click any artwork to see it larger, or right-click to reset it.</p>

      <ul className="slots">
        {slots.map((slot) => {
          const sources = sourcesFor(entry, slot);
          return (
            <li key={slot.asset_type} className="slot">
              {/* A button again. It stopped being one when left-clicking jumped to that browsing
                  tab — the view navigating away by itself when all you did was look at something.
                  Enlarging in place is not that, and a button is what gets this to the keyboard. */}
              <button
                type="button"
                className={`slot-art slot-art-${slot.asset_type}${
                  sources.length === 0 ? ' slot-art-flat' : ''
                }`}
                // Nothing to enlarge when every rung of the ladder is empty. Left as a no-op
                // rather than `disabled`, because a disabled button fires no mouse events at all
                // in Chromium — which would take the right-click menu with it.
                onClick={() => sources.length > 0 && setPreview(slot.asset_type)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({ x: e.clientX, y: e.clientY, slot });
                }}
                title={slot.label}
              >
                <ArtImage
                  sources={sources}
                  alt=""
                  fallback={<span className="art-none">No artwork</span>}
                />
                {busy === slot.asset_type && <span className="applying">Resetting…</span>}
              </button>
              <span className="slot-name">{slot.label}</span>
              <span className={`slot-state ${slot.custom_art ? 'slot-custom' : ''}`}>
                {state(slot)}
              </span>
            </li>
          );
        })}
      </ul>

      {previewSlot && (
        <ArtPreview
          slot={previewSlot}
          sources={sourcesFor(entry, previewSlot)}
          onClose={() => setPreview(null)}
        />
      )}

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
              <span className="menu-note">You haven&rsquo;t set artwork here.</span>
            </div>
          )}
        </ContextMenu>
      )}
    </>
  );
}

/**
 * One slot's artwork, as large as the window allows.
 *
 * It re-walks the same `sources` ladder rather than being handed whichever rung the tile settled
 * on: that index is private to `ArtImage`, and a second walk lands on the same rung out of the
 * browser cache. Artwork is shown at its own size and never scaled up — a 32-pixel icon blown up
 * to fill the frame is a worse look at it, not a better one.
 */
function ArtPreview({
  slot,
  sources,
  onClose,
}: {
  slot: AssetSlot;
  sources: string[];
  onClose: () => void;
}) {
  // Measured off the image that actually loaded rather than taken from anywhere else: what is on
  // disk is whatever was applied, and Steam's own defaults in particular are often smaller than
  // the slot suggests. Seeing the number is the difference between "this looks soft" and knowing
  // there is a larger copy worth going to find.
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  const ladder = sources.join('|');
  useEffect(() => setSize(null), [ladder]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      // The identity test, not `stopPropagation` on the child: a click that started on the picture
      // and ended on the backdrop is a drag, not a dismissal.
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="preview" role="dialog" aria-modal="true" aria-label={slot.label}>
        <div className={`preview-art preview-art-${slot.asset_type}`}>
          <ArtImage
            sources={sources}
            alt={slot.label}
            fallback={<span className="art-none">No artwork</span>}
            onLoad={(e) =>
              setSize({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
            }
          />
        </div>
        <div className="preview-foot">
          <span className="slot-name">{slot.label}</span>
          {/* Zero would mean the browser has not decoded it yet, not a zero-pixel image. */}
          {size && size.w > 0 && (
            <span className="dims">
              {size.w}×{size.h}
            </span>
          )}
          <span className={`slot-state ${slot.custom_art ? 'slot-custom' : ''}`}>
            {state(slot)}
          </span>
          <button type="button" className="ghost" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
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
  if (slot.custom_art) return 'Yours';
  if (slot.steam_art) return 'Steam default';
  return 'None';
}

