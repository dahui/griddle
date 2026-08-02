/**
 * The presentational pieces of the asset browser: one tile, one tab, and the two plain buttons.
 *
 * None of them touch the browser's state — they take what they render and a callback. Split out
 * so the container next door is about fetching and layout rather than about markup.
 */
import { isVideoPreview } from '@griddle/shared';
import type { Asset } from '../../api';
import { Flags } from '../../components';
import { useFocusGridItem, useFocusItem } from '../../focus';

/**
 * One asset's preview.
 *
 * Animated artwork's thumbnail is a **`.webm` video**, not an image, and an `<img>` renders
 * it as a broken-image icon — indistinguishable from missing artwork. 12% of Cyberpunk 2077's
 * capsules are affected. See `isVideoPreview` for why the extension, not the mime, is the test.
 *
 * The `<video>` needs `muted` for `autoPlay` to be allowed at all, and the CSP already carries
 * `media-src https://cdn2.steamgriddb.com` — someone anticipated this and the renderer did not
 * catch up.
 *
 * `full` switches from the thumbnail to the asset itself, for the details modal. It stays opt-in
 * because a grid of full-size assets is tens of megabytes, which is exactly what the thumbnails
 * exist to avoid — and an animated asset's thumbnail is the `.webm`, so the two differ in kind
 * and not only in size.
 */
export function AssetPreview({ asset, full = false }: { asset: Asset; full?: boolean }) {
  const src = full ? asset.url : (asset.thumb ?? asset.url);
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
  // Never lazy at full size: it is the only thing on screen and the one the user asked to see.
  return <img src={src} alt="" loading={full ? 'eager' : 'lazy'} />;
}

/** The way out. Its own section, above the tabs, and the first thing the pad reaches going up. */
export function BackButton({ onClick }: { onClick: () => void }) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('back', 0, 0);
  return (
    <button ref={ref} type="button" className={`ghost${focused ? ' focused' : ''}`} onClick={onClick}>
      ← Library
    </button>
  );
}

export function AssetTab({
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

/**
 * Tile size, as a pair of stepper buttons rather than a slider.
 *
 * `<input type="range">` is the obvious control and is the one thing a controller cannot drive
 * here: left and right belong to the focus model, so a focused slider either ignores them or
 * swallows them and traps the cursor. The library's sort control went the same way, from a native
 * `<select>` to plain buttons, for the same reason.
 *
 * Each button disables itself at its end of the range, so a press that would do nothing is
 * visibly unavailable instead of silently ignored — which on a pad is indistinguishable from the
 * input not arriving.
 */
export function ZoomControl({
  value,
  min,
  max,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (direction: 1 | -1) => void;
}) {
  const out = useFocusItem<HTMLButtonElement>('zoom', 0, 0);
  const inn = useFocusItem<HTMLButtonElement>('zoom', 0, 1);
  // A percentage of the type's own range, because the underlying rem width means nothing to
  // anyone and differs per tab — 22rem is the *smallest* a hero gets and the largest a capsule.
  const percent = Math.round(((value - min) / (max - min)) * 100);
  return (
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
      <span className="zoom-value" aria-live="polite">
        {percent}%
      </span>
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
}

export function LoadMore({ onClick }: { onClick: () => void }) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('load-more', 0, 0);
  return (
    <button ref={ref} type="button" className={`ghost${focused ? ' focused' : ''}`} onClick={onClick}>
      Load more
    </button>
  );
}

/**
 * One piece of candidate artwork.
 *
 * `disabled` is only on the tile actually being applied. It used to be on **every** tile
 * whenever any apply was in flight, which is invisible with a mouse but destroys keyboard and pad
 * navigation outright: a disabled button cannot hold focus, so the grid would empty out
 * mid-action and focus would be flung elsewhere. `aria-busy` on the container carries the "work
 * in progress" meaning instead, and a second click is guarded by the apply path itself.
 */
export function AssetTile({
  index,
  asset,
  label,
  applying,
  anyApplying,
  onApply,
  onDetails,
}: {
  index: number;
  asset: Asset;
  label: string;
  applying: boolean;
  anyApplying: boolean;
  onApply: () => void;
  onDetails: () => void;
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
        // Right-click opens the details, matching the Current tab, where right-click is already
        // "the other thing you can do to this artwork". The pad reaches it for free: the `menu`
        // action synthesises a contextmenu event on whatever is focused.
        onContextMenu={(e) => {
          e.preventDefault();
          onDetails();
        }}
        title={`Apply this ${label} — right-click for details`}
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
