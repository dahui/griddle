/**
 * One candidate asset, shown large with everything the API knows about it.
 *
 * The grid tile has room for an author and a size and nothing else, which is not enough to choose
 * between two capsules that look alike at 150px — style, format, and whether the thing is
 * animated are all decided here.
 *
 * **Opening this does not apply anything.** Left-click and the pad's accept button still apply
 * directly from the grid, because that is the fast path and the documented walkthrough; details
 * hang off right-click, which the pad's `menu` button synthesises on the focused element.
 */
import { assetPageUrl, ASSET_LABEL, STYLE_LABEL, type AssetType } from '@griddle/shared';
import { api, type Asset } from '../../api';
import { Flags, useErrorToast } from '../../components';
import { FocusScope, useFocusItem } from '../../focus';
import { AssetPreview } from './tiles';

/** A metadata row, rendered only when there is something to put in it. */
function Fact({ label, value, wide }: { label: string; value: string | null; wide?: boolean }) {
  if (!value) return null;
  return (
    <div className={wide ? 'fact fact-wide' : 'fact'}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

/**
 * Human wording for the raw `mime`.
 *
 * `image/vnd.microsoft.icon` in particular is worth translating: it is the one format Steam wants
 * for a shortcut icon, and nobody reads that string as "ICO".
 */
const MIME_LABEL: Record<string, string> = {
  'image/png': 'PNG',
  'image/jpeg': 'JPEG',
  'image/webp': 'WebP',
  'image/gif': 'GIF',
  'image/vnd.microsoft.icon': 'ICO',
};

export function AssetDetails({
  asset,
  assetType,
  applying,
  onApply,
  onClose,
}: {
  asset: Asset;
  assetType: AssetType;
  applying: boolean;
  onApply: () => void;
  onClose: () => void;
}) {
  const toastError = useErrorToast();
  const apply = useFocusItem<HTMLButtonElement>('details-actions', 0, 0);
  const open = useFocusItem<HTMLButtonElement>('details-actions', 0, 1);
  const close = useFocusItem<HTMLButtonElement>('details-actions', 0, 2);

  // Zero is legal for icons and means "not reported", not a zero-pixel image.
  const size = asset.width > 0 && asset.height > 0 ? `${asset.width}×${asset.height}` : null;
  const format = asset.mime ? (MIME_LABEL[asset.mime] ?? asset.mime) : null;

  return (
    <FocusScope name="details" onBack={onClose}>
      <div
        className="modal-backdrop"
        role="presentation"
        // Identity, not `stopPropagation` on the child: a click that starts on the picture and
        // ends on the backdrop is a drag, not a dismissal. Same reasoning as the current-art
        // preview next door.
        onClick={(e) => {
          if (e.target === e.currentTarget) onClose();
        }}
      >
        <div
          className="preview details"
          role="dialog"
          aria-modal="true"
          aria-label={`${ASSET_LABEL[assetType]} by ${asset.author.name || 'unknown'}`}
        >
          <div className={`preview-art preview-art-${assetType}`}>
            {/* The full asset, not the thumbnail the grid shows: seeing it at its own size is
                the whole reason to open this. */}
            <AssetPreview asset={asset} full />
          </div>

          <dl className="facts">
            <Fact label="By" value={asset.author.name || 'unknown'} />
            <Fact label="Size" value={size} />
            <Fact label="Format" value={format} />
            <Fact label="Style" value={asset.style ? (STYLE_LABEL[asset.style] ?? asset.style) : null} />
            <Fact label="Language" value={asset.language} />
            {/* Votes are the only quality signal SteamGridDB exposes, and a score with no counts
                behind it is unreadable — 100% from one vote is not 100% from two hundred. */}
            <Fact
              label="Votes"
              value={
                asset.upvotes + asset.downvotes > 0
                  ? `${asset.upvotes} up · ${asset.downvotes} down`
                  : null
              }
            />
            <Fact label="Notes" value={asset.notes} wide />
          </dl>

          <Flags asset={asset} />

          <div className="details-actions">
            <button
              ref={apply.ref}
              type="button"
              // No class: the bare button *is* the accent-filled one. `.ghost` is the secondary.
              className={apply.focused ? 'focused' : undefined}
              disabled={applying}
              // The scope takes whatever is `autoFocus`ed, so this is what a pad or keyboard
              // lands on. Apply is the action the user opened this to take, and it is reversible
              // — a reset is one right-click away on the Current tab.
              autoFocus
              onClick={onApply}
            >
              {applying ? 'Applying…' : `Apply this ${ASSET_LABEL[assetType].toLowerCase()}`}
            </button>
            <button
              ref={open.ref}
              type="button"
              className={`ghost${open.focused ? ' focused' : ''}`}
              // Opens in the real browser, not in this window: a Tauri webview ignores
              // `target="_blank"`, and `browser::open` is allowlisted to steamgriddb.com so a
              // malformed URL is refused rather than handed to the shell.
              onClick={() => {
                api.openUrl(assetPageUrl(assetType, asset.id)).catch(toastError);
              }}
            >
              View on SteamGridDB
            </button>
            <button
              ref={close.ref}
              type="button"
              className={`ghost${close.focused ? ' focused' : ''}`}
              onClick={onClose}
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </FocusScope>
  );
}
