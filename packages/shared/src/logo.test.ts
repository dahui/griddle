import { describe, expect, test } from 'bun:test';
import fixtures from '../fixtures/logo-positions.json';
import {
  DEFAULT_LOGO_POSITION,
  DPAD_CLAMP,
  MOUSE_CLAMP,
  PINNED_POSITIONS,
  STEP_MAX,
  STEP_MIN,
  isHorizontallyCentered,
  logoPositionToCss,
  nextPinnedPosition,
  parseLogoPosition,
  rampStep,
  resizeByDpad,
  resizeByMouse,
  toLogoPositionForApp,
  type LogoPosition,
  type PinnedPosition,
} from './logo';

describe('logoPositionToCss (shared golden table)', () => {
  // The same fixture file drives the Rust tests. If these two ever disagree, the desktop GUI
  // and the Big Picture UI would place logos differently for the same stored position.
  for (const c of fixtures.cases) {
    test(`${c.pin} w=${c.w} h=${c.h}`, () => {
      const css = logoPositionToCss({
        pinnedPosition: c.pin as PinnedPosition,
        nWidthPct: c.w,
        nHeightPct: c.h,
      });
      expect(css.top).toBeCloseTo(c.top, 6);
      expect(css.left).toBeCloseTo(c.left, 6);
    });
  }

  test('the fixture table covers every anchor', () => {
    const covered = new Set(fixtures.cases.map((c) => c.pin));
    expect([...covered].sort()).toEqual([...PINNED_POSITIONS].sort());
  });
});

describe('anchors', () => {
  test('there are exactly five, and no right-hand anchors exist', () => {
    expect(PINNED_POSITIONS).toHaveLength(5);
    expect(PINNED_POSITIONS).not.toContain('BottomRight' as PinnedPosition);
    expect(PINNED_POSITIONS).not.toContain('UpperRight' as PinnedPosition);
  });

  test('cycle order wraps', () => {
    const seen: PinnedPosition[] = [];
    let pin: PinnedPosition = 'BottomLeft';
    for (let i = 0; i < 5; i++) {
      seen.push(pin);
      pin = nextPinnedPosition(pin);
    }
    expect(seen).toEqual(['BottomLeft', 'UpperLeft', 'UpperCenter', 'CenterCenter', 'BottomCenter']);
    expect(pin).toBe('BottomLeft');
  });

  test('centered anchors are identified correctly', () => {
    expect(isHorizontallyCentered('UpperCenter')).toBe(true);
    expect(isHorizontallyCentered('CenterCenter')).toBe(true);
    expect(isHorizontallyCentered('BottomCenter')).toBe(true);
    expect(isHorizontallyCentered('UpperLeft')).toBe(false);
    expect(isHorizontallyCentered('BottomLeft')).toBe(false);
  });
});

describe('serialization', () => {
  test('wraps with nVersion 1, matching what Valve stringifies', () => {
    expect(toLogoPositionForApp(DEFAULT_LOGO_POSITION)).toEqual({
      nVersion: 1,
      logoPosition: { pinnedPosition: 'BottomLeft', nWidthPct: 50, nHeightPct: 50 },
    });
  });

  test('default is BottomLeft 50/50 — what a logo apply writes when nothing is stored', () => {
    expect(DEFAULT_LOGO_POSITION).toEqual({
      pinnedPosition: 'BottomLeft',
      nWidthPct: 50,
      nHeightPct: 50,
    });
  });

  test('parses both the wrapped and bare shapes', () => {
    const bare = { pinnedPosition: 'CenterCenter', nWidthPct: 40, nHeightPct: 30 };
    expect(parseLogoPosition(bare)).toEqual(bare as LogoPosition);
    expect(parseLogoPosition({ nVersion: 1, logoPosition: bare })).toEqual(bare as LogoPosition);
  });

  test('rejects junk rather than guessing', () => {
    // A hand-edited <appid>.json should degrade to "no custom position", not to a wrong one.
    expect(parseLogoPosition(null)).toBeNull();
    expect(parseLogoPosition('nope')).toBeNull();
    expect(parseLogoPosition({})).toBeNull();
    expect(parseLogoPosition({ pinnedPosition: 'BottomRight', nWidthPct: 50, nHeightPct: 50 })).toBeNull();
    expect(parseLogoPosition({ pinnedPosition: 'BottomLeft', nWidthPct: '50', nHeightPct: 50 })).toBeNull();
    expect(parseLogoPosition({ pinnedPosition: 'BottomLeft', nWidthPct: NaN, nHeightPct: 50 })).toBeNull();
  });
});

describe('resize', () => {
  const mid: LogoPosition = { pinnedPosition: 'BottomLeft', nWidthPct: 50, nHeightPct: 50 };

  test('d-pad up/down resize height, left/right resize width', () => {
    expect(resizeByDpad(mid, 'up', 1).nHeightPct).toBe(51);
    expect(resizeByDpad(mid, 'down', 1).nHeightPct).toBe(49);
    expect(resizeByDpad(mid, 'right', 1).nWidthPct).toBe(51);
    expect(resizeByDpad(mid, 'left', 1).nWidthPct).toBe(49);
  });

  test('d-pad clamps to [0.01, 100]', () => {
    const tiny: LogoPosition = { ...mid, nWidthPct: 0.01, nHeightPct: 0.01 };
    expect(resizeByDpad(tiny, 'left', 5).nWidthPct).toBe(DPAD_CLAMP.min);
    expect(resizeByDpad(tiny, 'down', 5).nHeightPct).toBe(DPAD_CLAMP.min);
    const full: LogoPosition = { ...mid, nWidthPct: 100, nHeightPct: 100 };
    expect(resizeByDpad(full, 'right', 5).nWidthPct).toBe(DPAD_CLAMP.max);
    expect(resizeByDpad(full, 'up', 5).nHeightPct).toBe(DPAD_CLAMP.max);
  });

  test('mouse clamps to the tighter [10, 100]', () => {
    expect(resizeByMouse(mid, -100, -100)).toEqual({ ...mid, nWidthPct: MOUSE_CLAMP.min, nHeightPct: MOUSE_CLAMP.min });
    expect(resizeByMouse(mid, 100, 100)).toEqual({ ...mid, nWidthPct: MOUSE_CLAMP.max, nHeightPct: MOUSE_CLAMP.max });
  });

  test('centered anchors resize horizontally at 2x', () => {
    const left = resizeByMouse({ ...mid, pinnedPosition: 'BottomLeft' }, 5, 0);
    const center = resizeByMouse({ ...mid, pinnedPosition: 'BottomCenter' }, 5, 0);
    expect(left.nWidthPct).toBe(55);
    expect(center.nWidthPct).toBe(60);
    // Vertical is unaffected by the horizontal centering.
    expect(resizeByMouse({ ...mid, pinnedPosition: 'CenterCenter' }, 0, 5).nHeightPct).toBe(55);
  });

  test('step ramps from fine to coarse and stops at the cap', () => {
    let step = STEP_MIN;
    for (let i = 0; i < 50; i++) step = rampStep(step);
    expect(step).toBe(STEP_MAX);
    expect(rampStep(STEP_MIN)).toBeGreaterThan(STEP_MIN);
  });

  test('property: repeated d-pad input never escapes the clamp', () => {
    let pos = mid;
    let step = STEP_MIN;
    const dirs = ['up', 'down', 'left', 'right'] as const;
    for (let i = 0; i < 500; i++) {
      pos = resizeByDpad(pos, dirs[i % 4]!, step);
      step = rampStep(step);
      expect(pos.nWidthPct).toBeGreaterThanOrEqual(DPAD_CLAMP.min);
      expect(pos.nWidthPct).toBeLessThanOrEqual(DPAD_CLAMP.max);
      expect(pos.nHeightPct).toBeGreaterThanOrEqual(DPAD_CLAMP.min);
      expect(pos.nHeightPct).toBeLessThanOrEqual(DPAD_CLAMP.max);
    }
  });

  test('property: css output stays on-canvas for every anchor and size', () => {
    for (const pin of PINNED_POSITIONS) {
      for (let w = 1; w <= 100; w += 7) {
        for (let h = 1; h <= 100; h += 7) {
          const css = logoPositionToCss({ pinnedPosition: pin, nWidthPct: w, nHeightPct: h });
          expect(css.top).toBeGreaterThanOrEqual(0);
          expect(css.left).toBeGreaterThanOrEqual(0);
          // The logo's far edge must not run past the container.
          expect(css.top + h).toBeLessThanOrEqual(100.000001);
          expect(css.left + w).toBeLessThanOrEqual(100.000001);
        }
      }
    }
  });
});
