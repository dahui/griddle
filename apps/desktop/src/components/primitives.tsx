/**
 * The plain pieces: images with fallbacks, an inline error, an external link, the sticky bar,
 * and the two one-liners every view uses.
 *
 * Nothing here owns state beyond its own element.
 */
import {
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type SyntheticEvent,
} from 'react';
import { createPortal } from 'react-dom';
import { api, asUiError, type UiError } from '../api';
import { useFocusItem } from '../focus';
import { NavSlotCtx } from '../navSlot';

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
  // Registered in its own section, and that is not a detail. An `ErrorNote` with a retry
  // button *replaces the entire view* in Library, AssetBrowser and CurrentAssets — so if it were
  // not reachable, hitting an error would leave a controller with nothing to press and no way
  // back. z13gui learned the same thing about its error bar's dismiss button.
  const { ref, focused } = useFocusItem<HTMLButtonElement>('error', 0, 0);
  return (
    <div className={`note ${error.kind === 'no_api_key' ? 'note-info' : 'note-bad'}`}>
      <p className="note-message">{error.message}</p>
      {error.action && <p className="note-action">{error.action}</p>}
      {onRetry && (
        <button
          ref={ref}
          type="button"
          className={`ghost${focused ? ' focused' : ''}`}
          onClick={onRetry}
        >
          Try again
        </button>
      )}
    </div>
  );
}

/**
 * A link that opens in the user's real browser.
 *
 * A plain `<a target="_blank">` **silently does nothing** in a Tauri webview: there is no
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
  section = 'key',
  row = 0,
  col = 0,
  className,
}: {
  href: string;
  children: ReactNode;
  onError?: (e: UiError) => void;
  section?: string;
  row?: number;
  col?: number;
  /**
   * Extra classes, so a link can be presented as a button where it is the primary action.
   *
   * It stays an `ExternalLink` rather than becoming a `FocusButton` that calls `api.openUrl`:
   * there is one allowlisted path out to a browser and it should have one call site.
   */
  className?: string;
}) {
  const { ref, focused } = useFocusItem<HTMLAnchorElement>(section, row, col);
  return (
    <a
      ref={ref}
      href={href}
      className={[className, focused ? 'focused' : null].filter(Boolean).join(' ') || undefined}
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
 * **`position: sticky` cannot tell you that it is stuck**, and the styling has to know —
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
 * A plain button that registers itself in the focus grid.
 *
 * Most controls need nothing more than this. The bespoke wrappers elsewhere exist only where the
 * button also carries view-specific classes or state (`.tab.active`, the asset tiles); anything
 * that is just a button at a known spot should use this.
 */
export function FocusButton({
  section,
  row,
  col,
  className,
  disabled,
  autoFocus,
  onClick,
  children,
}: {
  section: string;
  row: number;
  col: number;
  className?: string;
  disabled?: boolean;
  autoFocus?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>(section, row, col);
  return (
    <button
      ref={ref}
      type="button"
      className={[className, focused ? 'focused' : null].filter(Boolean).join(' ') || undefined}
      disabled={disabled}
      autoFocus={autoFocus}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/**
 * An on/off switch for a setting that takes effect the moment it moves.
 *
 * A switch, not a checkbox, and the difference is not decoration: a checkbox is a choice that
 * something *else* will act on when you press OK, and a switch is the action. Griddle's settings
 * write on change, so every preference here is the second kind. The one checkbox left in this
 * area — "don't ask again", inside the start-Steam dialog — is genuinely the first kind, since
 * nothing is written until a button is pressed.
 *
 * Still a real `<input type="checkbox">` underneath, with `role="switch"` over the top, so the
 * keyboard, the pad and a screen reader all get the behaviour they already know.
 */
export function Switch({
  section,
  row,
  col = 0,
  checked,
  disabled,
  onChange,
  children,
}: {
  section: string;
  row: number;
  col?: number;
  checked: boolean;
  disabled?: boolean;
  onChange: () => void;
  children: ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLInputElement>(section, row, col);
  return (
    <label className={`switch${focused ? ' focused' : ''}`}>
      <input
        ref={ref}
        type="checkbox"
        role="switch"
        checked={checked}
        disabled={disabled}
        onChange={onChange}
      />
      <span>{children}</span>
    </label>
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

/**
 * Tile size for a grid of artwork, as a pair of stepper buttons rather than a slider.
 *
 * `<input type="range">` is the obvious control and is the one thing a controller cannot drive
 * here: left and right belong to the focus model, so a focused slider either ignores them or
 * swallows them and traps the cursor. The library's sort control went the same way, from a native
 * `<select>` to plain buttons, for the same reason.
 *
 * Each button disables itself at its end of the range, so a press that would do nothing is
 * visibly unavailable instead of silently ignored — which on a pad is indistinguishable from the
 * input not arriving.
 *
 * **There is no numeric readout.** It briefly showed a percentage of the target's own min–max
 * window, which is a number with no meaning outside this control: "25%" at a perfectly ordinary
 * size invites the question of what 100% would be, and there is no answer worth giving. The grid
 * resizing under the press is the feedback, and it is immediate and unambiguous.
 *
 * `section`/`firstCol` are parameters because this now sits in three different toolbars, each its
 * own focus row — the browser's sticky bar, the library's, and the Current tab's.
 */
export function ZoomControl({
  value,
  min,
  max,
  section,
  firstCol,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  section: string;
  firstCol: number;
  onChange: (direction: 1 | -1) => void;
}) {
  const out = useFocusItem<HTMLButtonElement>(section, 0, firstCol);
  const inn = useFocusItem<HTMLButtonElement>(section, 0, firstCol + 1);
  const slot = useContext(NavSlotCtx);
  const control = (
    <div className="zoom" role="group" aria-label="Tile size">
      <span className="zoom-label">Size</span>
      <button
        ref={out.ref}
        type="button"
        className={`ghost zoom-step${out.focused ? ' focused' : ''}`}
        disabled={value <= min}
        onClick={() => onChange(-1)}
        title="Smaller tiles"
        aria-label="Smaller tiles"
      >
        −
      </button>
      <button
        ref={inn.ref}
        type="button"
        className={`ghost zoom-step${inn.focused ? ' focused' : ''}`}
        disabled={value >= max}
        onClick={() => onChange(1)}
        title="Larger tiles"
        aria-label="Larger tiles"
      >
        +
      </button>
    </div>
  );
  // Rendered into the nav row, not where it is written. In the toolbar it had to compete with
  // the scope switcher, the filter box and the sort group for one row, and no amount of restyling
  // made a two-button stepper sit right among them. The nav row is half empty and its
  // `space-between` was waiting for exactly this.
  return slot ? createPortal(control, slot) : null;
}
