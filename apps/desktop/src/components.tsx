/** Small shared pieces. Kept together because none of them is big enough to earn a file. */
import { useEffect, useRef, useState, type ReactNode, type SyntheticEvent } from 'react';
import { api, asUiError, type UiError } from './api';

/**
 * An image with fallbacks, tried in order until one loads.
 *
 * Artwork comes from up to three places — the user's custom art, Steam's local cache, and
 * Steam's CDN — and which of them exist varies per game. Rather than ask the backend to decide,
 * the ladder is walked in the browser, where a failed load is already observable.
 *
 * Two details make this correct rather than merely plausible:
 *
 * - The `index >= sources.length` terminator. Without an explicit end, the last `onError`
 *   re-renders the same failing `src` and the browser retries it forever.
 * - `key={sources[index]}`. React reuses a DOM node when only `src` changes, and a node that has
 *   already errored can keep its error state, so the next rung never gets a real attempt.
 */
export function ArtImage({
  sources,
  alt,
  fallback,
  onLoad,
}: {
  sources: string[];
  alt: string;
  fallback: ReactNode;
  /** Fires for whichever rung actually loaded — `naturalWidth`/`naturalHeight` are real there. */
  onLoad?: (e: SyntheticEvent<HTMLImageElement>) => void;
}) {
  const [index, setIndex] = useState(0);
  const ladder = sources.join('|');

  // A different game (or asset type) means a different ladder, which has to restart from the
  // top — otherwise a card scrolled into a position that previously failed stays blank.
  useEffect(() => setIndex(0), [ladder]);

  if (index >= sources.length) return <>{fallback}</>;
  return (
    <img
      key={sources[index]}
      src={sources[index]}
      alt={alt}
      loading="lazy"
      onLoad={onLoad}
      onError={() => setIndex((i) => i + 1)}
    />
  );
}

/**
 * An error the user can act on.
 *
 * The `action` line is the point of the whole error design — most failures here are
 * environmental (Steam closed, no key, port taken), and for those, what to do next is more
 * useful than what went wrong.
 */
export function ErrorNote({ error, onRetry }: { error: UiError; onRetry?: () => void }) {
  return (
    <div className={`note ${error.kind === 'no_api_key' ? 'note-info' : 'note-bad'}`}>
      <p className="note-message">{error.message}</p>
      {error.action && <p className="note-action">{error.action}</p>}
      {onRetry && (
        <button type="button" className="ghost" onClick={onRetry}>
          Try again
        </button>
      )}
    </div>
  );
}

/**
 * A link that opens in the user's real browser.
 *
 * 🔴 A plain `<a target="_blank">` **silently does nothing** in a Tauri webview: there is no
 * browser chrome and no new window to open into. It still renders as a link, which is what made
 * the API-key link look merely unresponsive rather than unimplemented.
 *
 * The `href` is kept so the address shows on hover and "copy link" works; the click is handled
 * by the backend, which only opens allowlisted URLs.
 */
export function ExternalLink({
  href,
  children,
  onError,
}: {
  href: string;
  children: ReactNode;
  onError?: (e: UiError) => void;
}) {
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        void api.openUrl(href).catch((err: unknown) => onError?.(asUiError(err)));
      }}
    >
      {children}
    </a>
  );
}

export function Spinner({ label }: { label: string }) {
  return (
    <div className="spinner" role="status">
      <span className="dot" />
      {label}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty">{children}</div>;
}

/**
 * A right-click menu anchored at the cursor.
 *
 * Closes on Escape, on a click outside it, and on scroll — a menu that outlives what it points
 * at is worse than no menu, because the next click lands on an action the user has stopped
 * looking at. `position: fixed` so the coordinates are viewport-relative.
 *
 * 🔴 **A click *inside* the menu must not close it here.** The dismiss listener is on `window`
 * in the **capture** phase, so it runs before the click reaches the menu item's own handler. If
 * it closes unconditionally, React unmounts the item mid-dispatch and its `onClick` never fires
 * — every menu action silently does nothing, which is precisely how this shipped the first
 * time. Letting the item's own handler close the menu is what actually runs the action.
 */
export function ContextMenu({
  x,
  y,
  onClose,
  children,
}: {
  x: number;
  y: number;
  onClose: () => void;
  children: ReactNode;
}) {
  const menu = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const closeOutside = (e: Event) => {
      // A click on the menu is the menu being *used*, not dismissed.
      if (e.target instanceof Node && menu.current?.contains(e.target)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    // Scrolling moves what the menu points at, so it always dismisses — no inside/outside test.
    const onScroll = () => onClose();
    // `capture` so the menu still closes when something below stops propagation.
    window.addEventListener('click', closeOutside, true);
    window.addEventListener('contextmenu', closeOutside, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('click', closeOutside, true);
      window.removeEventListener('contextmenu', closeOutside, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  return (
    <div
      ref={menu}
      className="context-menu"
      role="menu"
      style={{
        // Keep the menu on screen when the click lands near an edge.
        left: Math.min(x, window.innerWidth - 260),
        top: Math.min(y, window.innerHeight - 120),
      }}
    >
      {children}
    </div>
  );
}

/** Content-warning chips. Shown because a user filtering for them wants to see which is which. */
export function Flags({ asset }: { asset: { nsfw: boolean; humor: boolean; epilepsy: boolean } }) {
  const flags = [
    asset.nsfw && 'Adult',
    asset.humor && 'Humor',
    asset.epilepsy && 'Epilepsy',
  ].filter(Boolean) as string[];
  if (flags.length === 0) return null;
  return (
    <span className="flags">
      {flags.map((f) => (
        <span key={f} className="flag">
          {f}
        </span>
      ))}
    </span>
  );
}
