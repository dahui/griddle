/**
 * Place a custom logo within the hero banner.
 *
 * Steam draws the logo over the hero and stores *where* in `grid/<appid>.json`. There are only
 * five anchors — no `BottomRight`, no `UpperRight`, no `CenterLeft` — and the size is a pair of
 * percentages of the hero area. All of that geometry lives in `@griddle/shared`, tested against
 * the same fixture the Rust side asserts on, so nothing here computes a coordinate.
 *
 * The preview is the point: this is a job done by eye, and the alternative — write, restart Steam,
 * look, adjust — is not a positioner. Applying happens on **Save**, not on every press, so the
 * live call is one round trip rather than one per nudge.
 *
 * Resizing is buttons rather than arrow keys, even though `resizeByDpad` exists for it: arrows
 * belong to the focus model here, and a control that swallowed them would trap the cursor. That
 * helper was written for the Big Picture UI, which had its own navigation and was cut.
 */
import { useState } from 'react';
import {
  DPAD_CLAMP,
  PINNED_POSITIONS,
  STEP_MAX,
  clamp,
  logoPositionToCss,
  type LogoPosition,
  type PinnedPosition,
} from '@griddle/shared';
import { api, asUiError, type UiError } from '../api';
import { ArtImage, ErrorNote, FocusButton } from '../components';
import { FocusScope, useFocusItem } from '../focus';

/** Human names for Steam's five anchors, which are PascalCase on the wire. */
const ANCHOR_LABEL: Record<PinnedPosition, string> = {
  BottomLeft: 'Bottom left',
  UpperLeft: 'Top left',
  UpperCenter: 'Top centre',
  CenterCenter: 'Centre',
  BottomCenter: 'Bottom centre',
};

/**
 * One press of a size button.
 *
 * `STEP_MAX` is the top of the held-input ramp `rampStep` produces, and it is the right size for
 * a discrete press too: fine enough to land where you want, coarse enough to cross the range
 * without a hundred clicks.
 */
const STEP = STEP_MAX;

export function LogoPositioner({
  appId,
  gameName,
  heroSources,
  logoSources,
  initial,
  fallback,
  onSaved,
  onClose,
}: {
  appId: number;
  gameName: string;
  heroSources: string[];
  logoSources: string[];
  /** What is stored today, or null when this app has never had a position. */
  initial: LogoPosition | null;
  /** What Steam gets by default, and what Reset restores. */
  fallback: LogoPosition;
  onSaved: (message: string) => void;
  onClose: () => void;
}) {
  const [position, setPosition] = useState<LogoPosition>(initial ?? fallback);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UiError | null>(null);

  const css = logoPositionToCss(position);

  function resize(axis: 'width' | 'height', direction: 1 | -1) {
    setPosition((p) => ({
      ...p,
      ...(axis === 'width'
        ? { nWidthPct: clamp(p.nWidthPct + STEP * direction, DPAD_CLAMP) }
        : { nHeightPct: clamp(p.nHeightPct + STEP * direction, DPAD_CLAMP) }),
    }));
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const moved = await api.setLogoPlacement(appId, position);
      onSaved(
        moved.method === 'live' ? 'Logo moved.' : 'Logo moved. Restart Steam to see it.',
      );
      onClose();
    } catch (e: unknown) {
      // Kept open on failure: the position the user built is in this component's state and
      // closing would throw it away along with the only thing that explains what went wrong.
      setError(asUiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <FocusScope name="logo-position" onBack={onClose}>
      <div
        className="modal-backdrop"
        role="presentation"
        onClick={(e) => {
          if (e.target === e.currentTarget) onClose();
        }}
      >
        <div className="modal logo-modal" role="dialog" aria-modal="true" aria-label="Logo position">
          <div className="modal-head">
            <h2>Logo position: {gameName}</h2>
          </div>

          {/*
            The hero at its real aspect ratio with the logo laid over it, both from the same
            ladders the Current tab uses. Percentages all the way down, so the preview is
            faithful at whatever size the window gives it.
          */}
          <div className="logo-stage">
            <ArtImage
              sources={heroSources}
              alt=""
              fallback={<span className="art-none">No hero artwork to position against</span>}
            />
            <div
              className="logo-ghost"
              style={{
                top: `${css.top}%`,
                left: `${css.left}%`,
                width: `${position.nWidthPct}%`,
                height: `${position.nHeightPct}%`,
              }}
            >
              <ArtImage sources={logoSources} alt="" fallback={<span className="art-none">No logo</span>} />
            </div>
          </div>

          <div className="logo-controls">
            <div className="logo-field">
              <span className="field-label">Anchor</span>
              <div className="tab-group">
                {PINNED_POSITIONS.map((pin, i) => (
                  <FocusButton
                    key={pin}
                    section="logo-anchor"
                    row={0}
                    col={i}
                    className={`tab${position.pinnedPosition === pin ? ' active' : ''}`}
                    onClick={() => setPosition((p) => ({ ...p, pinnedPosition: pin }))}
                  >
                    {ANCHOR_LABEL[pin]}
                  </FocusButton>
                ))}
              </div>
            </div>

            <SizeField
              label="Width"
              value={position.nWidthPct}
              col={0}
              onChange={(d) => resize('width', d)}
            />
            <SizeField
              label="Height"
              value={position.nHeightPct}
              col={2}
              onChange={(d) => resize('height', d)}
            />
          </div>

          {error && <ErrorNote error={error} />}

          <div className="modal-actions row">
            <FocusButton
              section="logo-actions"
              row={0}
              col={0}
              disabled={busy}
              autoFocus
              onClick={() => void save()}
            >
              {busy ? 'Saving…' : 'Save'}
            </FocusButton>
            <FocusButton
              section="logo-actions"
              row={0}
              col={1}
              className="ghost"
              disabled={busy}
              onClick={() => setPosition(fallback)}
            >
              Reset to default
            </FocusButton>
            <FocusButton
              section="logo-actions"
              row={0}
              col={2}
              className="ghost"
              disabled={busy}
              onClick={onClose}
            >
              Cancel
            </FocusButton>
          </div>
        </div>
      </div>
    </FocusScope>
  );
}

/**
 * One percentage with a stepper either side.
 *
 * The number *is* shown here, unlike the tile-size control: a logo's width as a percentage of the
 * hero is exactly what Steam stores, so it is a real quantity the user may want to match across
 * games rather than a position within an invented range.
 */
function SizeField({
  label,
  value,
  col,
  onChange,
}: {
  label: string;
  value: number;
  col: number;
  onChange: (direction: 1 | -1) => void;
}) {
  const down = useFocusItem<HTMLButtonElement>('logo-size', 0, col);
  const up = useFocusItem<HTMLButtonElement>('logo-size', 0, col + 1);
  return (
    <div className="logo-field">
      <span className="field-label">{label}</span>
      <div className="zoom">
        <button
          ref={down.ref}
          type="button"
          className={`ghost zoom-step${down.focused ? ' focused' : ''}`}
          disabled={value <= DPAD_CLAMP.min}
          onClick={() => onChange(-1)}
          aria-label={`Reduce ${label.toLowerCase()}`}
        >
          −
        </button>
        {/* One decimal: the step is 2%, but Steam's own stored values are fractional and a
            reset to one of them should not display as a rounded lie. */}
        <span className="logo-value">{value.toFixed(1)}%</span>
        <button
          ref={up.ref}
          type="button"
          className={`ghost zoom-step${up.focused ? ' focused' : ''}`}
          disabled={value >= DPAD_CLAMP.max}
          onClick={() => onChange(1)}
          aria-label={`Increase ${label.toLowerCase()}`}
        >
          +
        </button>
      </div>
    </div>
  );
}
