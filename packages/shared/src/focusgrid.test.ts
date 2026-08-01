import { describe, expect, test } from 'bun:test';
import {
  firstFocusable,
  flowPosition,
  move,
  nearest,
  type GridItem,
  type Layout,
} from './focusgrid';

/** `a1` means section `a`, row 1 — ids are readable so a failure names the position. */
const item = (section: string, row: number, col: number): GridItem => ({
  id: `${section}-${row}-${col}`,
  section,
  row,
  col,
});

/** A row of `count` items, as a horizontal bar would be. */
const bar = (section: string, count: number, row = 0): GridItem[] =>
  Array.from({ length: count }, (_, col) => item(section, row, col));

/** A wrapping grid of `count` tiles at `columns` wide. */
const grid = (section: string, count: number, columns: number): GridItem[] =>
  Array.from({ length: count }, (_, i) => {
    const { row, col } = flowPosition(i, columns);
    return item(section, row, col);
  });

describe('horizontal movement', () => {
  const layout: Layout = { items: bar('tabs', 3), sections: ['tabs'] };

  test('steps along the row', () => {
    expect(move(layout, 'tabs-0-0', 'right')).toBe('tabs-0-1');
    expect(move(layout, 'tabs-0-2', 'left')).toBe('tabs-0-1');
  });

  test('a non-flow section stops at its edges rather than wrapping', () => {
    // A tab bar that teleports from its last tab to its first reads as a misfire, not a feature.
    expect(move(layout, 'tabs-0-2', 'right')).toBeNull();
    expect(move(layout, 'tabs-0-0', 'left')).toBeNull();
  });

  test('a flow section continues into the next row, in reading order', () => {
    // The behaviour the asset grid needs: 7 tiles at 3 columns, so row 0 ends at col 2.
    const flowing: Layout = { items: grid('art', 7, 3), sections: ['art'], flow: ['art'] };
    expect(move(flowing, 'art-0-2', 'right')).toBe('art-1-0');
    expect(move(flowing, 'art-1-0', 'left')).toBe('art-0-2');
    // …but the very ends still stop. There is no row beyond the last.
    expect(move(flowing, 'art-2-0', 'right')).toBeNull();
    expect(move(flowing, 'art-0-0', 'left')).toBeNull();
  });
});

describe('vertical movement', () => {
  test('keeps the column when the target row is long enough', () => {
    const layout: Layout = { items: grid('art', 9, 3), sections: ['art'], flow: ['art'] };
    expect(move(layout, 'art-0-2', 'down')).toBe('art-1-2');
    expect(move(layout, 'art-2-1', 'up')).toBe('art-1-1');
  });

  test('clamps to the nearest column when the target row is shorter', () => {
    // 5 tiles at 3 columns: the last row holds cols 0 and 1 only. Down from col 2 must land
    // somewhere real rather than falling off the end of a short row.
    const layout: Layout = { items: grid('art', 5, 3), sections: ['art'], flow: ['art'] };
    expect(move(layout, 'art-0-2', 'down')).toBe('art-1-1');
  });

  test('crosses into the next section, entering at the nearest column', () => {
    const layout: Layout = {
      items: [...bar('toolbar', 4), ...grid('art', 6, 3)],
      sections: ['toolbar', 'art'],
      flow: ['art'],
    };
    expect(move(layout, 'toolbar-0-1', 'down')).toBe('art-0-1');
    // Column 3 does not exist in a 3-wide grid, so it clamps to the last one.
    expect(move(layout, 'toolbar-0-3', 'down')).toBe('art-0-2');
    // Back up, entering the toolbar's only row.
    expect(move(layout, 'art-0-2', 'up')).toBe('toolbar-0-2');
  });

  test('skips a section that has no items', () => {
    // Exactly the collapsed-filter-panel case: the section is still listed, but empty. It must
    // not swallow the keypress, or the grid below becomes unreachable from the toolbar.
    const layout: Layout = {
      items: [...bar('toolbar', 2), ...bar('grid', 2, 0)],
      sections: ['toolbar', 'filters', 'grid'],
    };
    expect(move(layout, 'toolbar-0-0', 'down')).toBe('grid-0-0');
  });

  test('stops at the outer edges', () => {
    const layout: Layout = { items: bar('only', 2), sections: ['only'] };
    expect(move(layout, 'only-0-0', 'up')).toBeNull();
    expect(move(layout, 'only-0-1', 'down')).toBeNull();
  });
});

describe('entry points', () => {
  const layout: Layout = {
    items: [...bar('toolbar', 2), ...grid('art', 4, 2)],
    sections: ['toolbar', 'art'],
    flow: ['art'],
  };

  test('nothing focused yet lands on the first item', () => {
    expect(move(layout, null, 'down')).toBe('toolbar-0-0');
    expect(firstFocusable(layout)).toBe('toolbar-0-0');
  });

  test('the first item skips leading empty sections', () => {
    const withEmpty: Layout = { ...layout, sections: ['ghost', 'toolbar', 'art'] };
    expect(firstFocusable(withEmpty)).toBe('toolbar-0-0');
  });

  test('an empty layout has nowhere to go rather than throwing', () => {
    const empty: Layout = { items: [], sections: ['a', 'b'] };
    expect(firstFocusable(empty)).toBeNull();
    expect(move(empty, null, 'down')).toBeNull();
  });

  test('a stale id recovers to the first item instead of freezing', () => {
    // If this returned null the pad would be unable to move at all, which is the one failure
    // mode worse than moving somewhere unexpected.
    expect(move(layout, 'art-9-9', 'up')).toBe('toolbar-0-0');
  });
});

describe('recovering from a vanished item', () => {
  test('stays in the same section, preferring the same row', () => {
    // "Reset filters" disappears the moment filters go back to their defaults, taking the focus
    // with it. Landing back at the top of the page for that would be jarring.
    const layout: Layout = {
      items: [...bar('toolbar', 3), ...bar('actions', 2, 0)],
      sections: ['toolbar', 'actions'],
    };
    const lost = item('actions', 0, 4);
    expect(nearest(layout, lost)).toBe('actions-0-1');
  });

  test('prefers a column on the same row over a nearer column one row away', () => {
    const layout: Layout = {
      items: [item('s', 0, 9), item('s', 1, 0)],
      sections: ['s'],
    };
    // Column distance says row 1 col 0 (distance 0); row distance must outrank it.
    expect(nearest(layout, item('s', 0, 0))).toBe('s-0-9');
  });

  test('falls back to the first item when the whole section is gone', () => {
    const layout: Layout = { items: bar('toolbar', 2), sections: ['toolbar'] };
    expect(nearest(layout, item('filters', 3, 1))).toBe('toolbar-0-0');
  });
});

describe('flowPosition', () => {
  test('lays tiles out in reading order', () => {
    expect(flowPosition(0, 4)).toEqual({ row: 0, col: 0 });
    expect(flowPosition(3, 4)).toEqual({ row: 0, col: 3 });
    expect(flowPosition(4, 4)).toEqual({ row: 1, col: 0 });
  });

  test('a nonsense column count degrades to a single column rather than dividing by zero', () => {
    // `measureColumns` returns 0 for a grid that has not been laid out yet, and this runs on
    // every render — an Infinity or NaN row index here would poison the whole layout.
    expect(flowPosition(2, 0)).toEqual({ row: 2, col: 0 });
    expect(flowPosition(2, -3)).toEqual({ row: 2, col: 0 });
  });
});
