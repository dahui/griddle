/** Small shared pieces. Kept together because none of them is big enough to earn a file. */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type SyntheticEvent,
} from 'react';
import { api, asUiError, type UiError } from './api';

// -- toasts ---------------------------------------------------------------------------------

type ToastKind = 'ok' | 'info' | 'bad';

interface NewToast {
  kind: ToastKind;
  message: string;
  /** The second line — what to do about it. Only failures usually have one. */
  action?: string | null;
}

interface Toast extends NewToast {
  id: number;
  life: number;
}

/** Long enough to read a sentence without being in the way. */
const TOAST_LIFE = 4000;
/** Failures get longer: they are unexpected, so they are read from a standing start. */
const TOAST_LIFE_BAD = 7000;

const ToastContext = createContext<(t: NewToast) => void>(() => undefined);

/**
 * Transient confirmations, bottom-centre.
 *
 * 🔴 **Not every message belongs here.** A toast is right when the user has just *done*
 * something and wants acknowledgement — applied artwork, reset a slot. It is wrong when the
 * message *is* the state of the view: the library's load failure renders instead of the list, so
 * fading it out would leave an empty screen with no explanation and nothing to retry. Those stay
 * as an inline {@link ErrorNote}.
 *
 * The rule: **if dismissing the message would leave the user with no idea what to do next, it
 * must not dismiss itself.**
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  // A counter rather than a timestamp or a random: two toasts raised in the same tick must not
  // collide, and React keys must be stable across renders.
  const nextId = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const notify = useCallback(
    (t: NewToast) => {
      const id = nextId.current++;
      const life = t.kind === 'bad' ? TOAST_LIFE_BAD : TOAST_LIFE;
      setToasts((prev) => [...prev, { ...t, id, life }]);
      setTimeout(() => dismiss(id), life);
    },
    [dismiss],
  );

  return (
    <ToastContext.Provider value={notify}>
      {children}
      {/* `aria-live` on the container, not the toast: a live region has to exist before the
          content arrives or a screen reader never announces it. */}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`toast toast-${t.kind}`}
            // The CSS animation fades in *and* out across exactly this span, so one timer drives
            // both the visuals and the removal. Two timers would drift apart.
            style={{ '--toast-life': `${t.life}ms` } as CSSProperties}
            onClick={() => dismiss(t.id)}
            title="Dismiss"
          >
            <span className="toast-message">{t.message}</span>
            {t.action && <span className="toast-action">{t.action}</span>}
          </button>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

/** Raise a transient message. See {@link ToastProvider} for when not to. */
export function useToast() {
  return useContext(ToastContext);
}

/** A {@link UiError} as a toast, for failures that do not stop the view working. */
export function useErrorToast() {
  const notify = useToast();
  return useCallback(
    (e: unknown) => {
      const ui = asUiError(e);
      notify({ kind: 'bad', message: ui.message, action: ui.action });
    },
    [notify],
  );
}

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

/**
 * A bar that sits in the page normally and pins itself to the top of the window once it would
 * otherwise scroll away.
 *
 * Infinite scroll is what makes this worth having: the further you browse, the more expensive
 * "scroll back to the top to go back" becomes, and that cost grows exactly as the list gets more
 * interesting.
 *
 * 🔴 **`position: sticky` cannot tell you that it is stuck**, and the styling has to know —
 * a bar that carries a shadow while sitting in the page looks like a mistake. Hence the probe:
 * a 1px element immediately above the bar, whose leaving the viewport *is* the pin. It is
 * rendered here rather than by the caller so the two cannot be separated.
 *
 * The observer is safe from the re-fire trap that has bitten this codebase twice: every callback
 * is acted on, nothing is skipped by a guard, and the probe genuinely crosses the boundary in
 * both directions.
 */
export function StickyBar({ className, children }: { className?: string; children: ReactNode }) {
  const probe = useRef<HTMLDivElement | null>(null);
  const [stuck, setStuck] = useState(false);

  useEffect(() => {
    const node = probe.current;
    if (!node) return undefined;
    const observer = new IntersectionObserver((entries) =>
      setStuck(!entries.some((e) => e.isIntersecting)),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  return (
    <>
      <div ref={probe} className="sticky-probe" aria-hidden="true" />
      <div className={`sticky-bar${stuck ? ' stuck' : ''}${className ? ` ${className}` : ''}`}>
        {children}
      </div>
    </>
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
