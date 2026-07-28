/**
 * Custom logo positioning.
 *
 * Steam stores this in `userdata/<id>/config/grid/<appid>.json`:
 *
 * ```json
 * {"nVersion":1,"logoPosition":{"pinnedPosition":"BottomLeft","nWidthPct":50,"nHeightPct":50}}
 * ```
 *
 * Confirmed against Valve's own shipped code, which calls
 * `SetCustomLogoPositionForApp(e.appid, JSON.stringify({nVersion:1, logoPosition:t}))`.
 * `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`
 *
 * # Two things that will bite
 *
 * **There are only five anchors.** No `BottomRight`, no `UpperRight`, no `CenterLeft`. The
 * enum below is exhaustive; anything else is rejected by Steam.
 *
 * **A custom logo with no stored position may not render at all.** So writing `<appid>_logo.png`
 * must also write an `<appid>.json` when none exists. The Decky plugin force-creates
 * `{BottomLeft, 50, 50}` for shortcuts for exactly this reason. `[VERIFIED-SOURCE]`
 */

export const PINNED_POSITIONS = [
  'BottomLeft',
  'UpperLeft',
  'UpperCenter',
  'CenterCenter',
  'BottomCenter',
] as const;

export type PinnedPosition = (typeof PINNED_POSITIONS)[number];

export interface LogoPosition {
  pinnedPosition: PinnedPosition;
  /** Percentage of the hero area's width, 0-100. */
  nWidthPct: number;
  /** Percentage of the hero area's height, 0-100. */
  nHeightPct: number;
}

export interface LogoPositionForApp {
  nVersion: number;
  logoPosition: LogoPosition;
}

/** What Steam gets when a logo is applied to an app that has no stored position. */
export const DEFAULT_LOGO_POSITION: LogoPosition = {
  pinnedPosition: 'BottomLeft',
  nWidthPct: 50,
  nHeightPct: 50,
};

export function toLogoPositionForApp(position: LogoPosition): LogoPositionForApp {
  return { nVersion: 1, logoPosition: position };
}

/**
 * CSS `top`/`left` percentages for a position.
 *
 * The centered anchors offset by half the *remaining* space, which is why they visually move
 * at twice the rate of a corner anchor when resized — see {@link resizeByMouse}.
 */
export function logoPositionToCss(position: LogoPosition): { top: number; left: number } {
  const { pinnedPosition: pin, nWidthPct: w, nHeightPct: h } = position;
  const centeredLeft = (100 - w) / 2;
  switch (pin) {
    case 'UpperLeft':
      return { top: 0, left: 0 };
    case 'BottomLeft':
      return { top: 100 - h, left: 0 };
    case 'UpperCenter':
      return { top: 0, left: centeredLeft };
    case 'CenterCenter':
      return { top: (100 - h) / 2, left: centeredLeft };
    case 'BottomCenter':
      return { top: 100 - h, left: centeredLeft };
  }
}

/** Cycle order for the "next anchor" action (the Y button in the positioner). */
export function nextPinnedPosition(current: PinnedPosition): PinnedPosition {
  const i = PINNED_POSITIONS.indexOf(current);
  return PINNED_POSITIONS[(i + 1) % PINNED_POSITIONS.length] as PinnedPosition;
}

/** True for anchors that grow in both directions horizontally. */
export function isHorizontallyCentered(pin: PinnedPosition): boolean {
  return pin === 'UpperCenter' || pin === 'CenterCenter' || pin === 'BottomCenter';
}

// -- Resize -----------------------------------------------------------------------------

/** D-pad resize clamps. Near-zero is permitted; the user can nudge back up. */
export const DPAD_CLAMP = { min: 0.01, max: 100 } as const;
/** Mouse-drag clamps. Tighter, because a drag can overshoot in a single frame. */
export const MOUSE_CLAMP = { min: 10, max: 100 } as const;

/** Step ramp for held D-pad input: starts fine for precision, accelerates for reach. */
export const STEP_MIN = 0.25;
export const STEP_MAX = 2.0;
const STEP_ACCEL = 0.25;

/** Grow the repeat step toward {@link STEP_MAX} while a direction is held. */
export function rampStep(current: number): number {
  return Math.min(STEP_MAX, current + STEP_ACCEL);
}

export function clamp(value: number, { min, max }: { min: number; max: number }): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * Apply one D-pad step. Up/down resize height, left/right resize width.
 *
 * Directions are inverted from what "arrow key moves the thing" would suggest, because the
 * positioner resizes rather than translates: the anchor decides where it sits.
 */
export function resizeByDpad(
  position: LogoPosition,
  direction: 'up' | 'down' | 'left' | 'right',
  step: number,
): LogoPosition {
  const next = { ...position };
  switch (direction) {
    case 'up':
      next.nHeightPct = clamp(position.nHeightPct + step, DPAD_CLAMP);
      break;
    case 'down':
      next.nHeightPct = clamp(position.nHeightPct - step, DPAD_CLAMP);
      break;
    case 'right':
      next.nWidthPct = clamp(position.nWidthPct + step, DPAD_CLAMP);
      break;
    case 'left':
      next.nWidthPct = clamp(position.nWidthPct - step, DPAD_CLAMP);
      break;
  }
  return next;
}

/**
 * Apply a mouse drag, in percentage-of-container deltas.
 *
 * Centered anchors move at 2x because the logo grows away from the anchor in both directions
 * at once — without this the handle visibly lags the cursor.
 */
export function resizeByMouse(
  position: LogoPosition,
  deltaWidthPct: number,
  deltaHeightPct: number,
): LogoPosition {
  const rate = isHorizontallyCentered(position.pinnedPosition) ? 2 : 1;
  return {
    ...position,
    nWidthPct: clamp(position.nWidthPct + deltaWidthPct * rate, MOUSE_CLAMP),
    nHeightPct: clamp(position.nHeightPct + deltaHeightPct, MOUSE_CLAMP),
  };
}

/** Narrow untrusted JSON (a hand-edited `<appid>.json`) to a usable position. */
export function parseLogoPosition(value: unknown): LogoPosition | null {
  if (typeof value !== 'object' || value === null) return null;
  const outer = value as Record<string, unknown>;
  const raw = ('logoPosition' in outer ? outer.logoPosition : outer) as Record<string, unknown>;
  if (typeof raw !== 'object' || raw === null) return null;

  const pin = raw.pinnedPosition;
  const w = raw.nWidthPct;
  const h = raw.nHeightPct;
  if (typeof pin !== 'string' || !PINNED_POSITIONS.includes(pin as PinnedPosition)) return null;
  if (typeof w !== 'number' || typeof h !== 'number') return null;
  if (!Number.isFinite(w) || !Number.isFinite(h)) return null;

  return { pinnedPosition: pin as PinnedPosition, nWidthPct: w, nHeightPct: h };
}
