/**
 * Spatial focus navigation — pure index maths over a registered set of items.
 *
 * A controller has no cursor, so the focus ring *is* the cursor and something has to decide what
 * "down" means from any given control. Tab order cannot: it is a single flat sequence, so in a
 * wrapping grid of artwork "down" would step one tile sideways rather than one row.
 *
 * The model is `(section, row, col)`, following z13gui's `internal/focusgrid`. Sections stack
 * vertically and are supplied in visual order; within a section, items sit at explicit row/column
 * indices. Moving off the top or bottom of a section crosses into the neighbouring one, entering
 * at the nearest column.
 *
 * **This module is deliberately DOM-free.** Everything here is arithmetic over plain objects,
 * which is why it can be tested exhaustively without a browser, a controller, or a rendered app —
 * and every awkward case below (a short target row, an empty section, a vanished item) is a test
 * rather than something to discover with a gamepad in hand.
 *
 * The DOM half — measuring how many columns a wrapping grid actually has, ordering sections by
 * document position, and moving real focus — lives in `apps/desktop/src/focus.tsx`.
 */

export type Direction = 'up' | 'down' | 'left' | 'right';

/** One focusable control, at a fixed position in its section's grid. */
export interface GridItem {
  readonly id: string;
  readonly section: string;
  readonly row: number;
  readonly col: number;
}

export interface Layout {
  readonly items: readonly GridItem[];
  /**
   * Section names in visual order, top to bottom.
   *
   * Items whose section is not listed are unreachable — that is the intended behaviour for a
   * collapsed `<details>` or a hidden filter group, whose contents must not be navigable while
   * they cannot be seen.
   */
  readonly sections: readonly string[];
  /**
   * Sections laid out as a wrapping grid, where moving past the end of a row continues at the
   * start of the next one.
   *
   * This is reading order, and it is what a wrapping grid of tiles looks like it should do. A
   * horizontal toolbar or tab bar is *not* a flow section: running off its right-hand end should
   * stop, not teleport.
   */
  readonly flow?: readonly string[];
}

/** Items in one section, sorted by row then column. */
function itemsIn(layout: Layout, section: string): GridItem[] {
  return layout.items
    .filter((i) => i.section === section)
    .sort((a, b) => a.row - b.row || a.col - b.col);
}

/** The occupied row indices, ascending. Rows may be sparse; nothing assumes 0..n. */
function rowsIn(items: readonly GridItem[]): number[] {
  return [...new Set(items.map((i) => i.row))].sort((a, b) => a - b);
}

function inRow(items: readonly GridItem[], row: number): GridItem[] {
  return items.filter((i) => i.row === row).sort((a, b) => a.col - b.col);
}

/**
 * The item nearest a target column.
 *
 * Used whenever movement enters a row it did not come from, which is the case that makes
 * clamping insufficient: the target row is often shorter than the one being left, and a plain
 * index lookup would fall off the end and land nowhere.
 */
function closestCol(candidates: readonly GridItem[], col: number): GridItem | null {
  let best: GridItem | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const item of candidates) {
    const distance = Math.abs(item.col - col);
    // Strictly less-than, so a tie keeps the lower column — leftward is the predictable bias.
    if (distance < bestDistance) {
      bestDistance = distance;
      best = item;
    }
  }
  return best;
}

/** The first reachable item: the earliest section that actually has one. */
export function firstFocusable(layout: Layout): string | null {
  for (const section of layout.sections) {
    const items = itemsIn(layout, section);
    if (items.length > 0) return items[0]?.id ?? null;
  }
  return null;
}

function horizontal(layout: Layout, current: GridItem, delta: 1 | -1): string | null {
  const all = itemsIn(layout, current.section);
  const row = inRow(all, current.row);
  const index = row.findIndex((i) => i.id === current.id);
  const sibling = row[index + delta];
  if (sibling) return sibling.id;

  // Off the end of the row. Only a flow section continues; everything else stops here.
  if (!(layout.flow ?? []).includes(current.section)) return null;

  const rows = rowsIn(all);
  const target = rows[rows.indexOf(current.row) + delta];
  if (target === undefined) return null;
  const next = inRow(all, target);
  return (delta > 0 ? next[0] : next[next.length - 1])?.id ?? null;
}

function vertical(layout: Layout, current: GridItem, delta: 1 | -1): string | null {
  const all = itemsIn(layout, current.section);
  const rows = rowsIn(all);
  const target = rows[rows.indexOf(current.row) + delta];
  if (target !== undefined) {
    return closestCol(inRow(all, target), current.col)?.id ?? null;
  }

  // Out of the section. Walk on until one has items — an empty section is skipped rather than
  // swallowing the keypress, which is what makes a hidden filter group harmless.
  const start = layout.sections.indexOf(current.section);
  for (let k = start + delta; k >= 0 && k < layout.sections.length; k += delta) {
    const name = layout.sections[k];
    if (name === undefined) continue;
    const items = itemsIn(layout, name);
    if (items.length === 0) continue;
    const entryRows = rowsIn(items);
    const entry = delta > 0 ? entryRows[0] : entryRows[entryRows.length - 1];
    if (entry === undefined) continue;
    return closestCol(inRow(items, entry), current.col)?.id ?? null;
  }
  return null;
}

/**
 * Where focus goes for one directional press.
 *
 * Returns `null` when there is nowhere to go, which the caller should treat as "stay put" rather
 * than "clear the focus" — running into the edge of the screen must not strand a controller with
 * nothing selected.
 */
export function move(layout: Layout, currentId: string | null, direction: Direction): string | null {
  if (currentId === null) return firstFocusable(layout);
  const current = layout.items.find((i) => i.id === currentId);
  // The current item is gone but the caller still thinks it is focused. Recovering to the first
  // item beats returning null, which would leave the pad unable to move at all.
  if (!current) return firstFocusable(layout);

  switch (direction) {
    case 'left':
      return horizontal(layout, current, -1);
    case 'right':
      return horizontal(layout, current, 1);
    case 'up':
      return vertical(layout, current, -1);
    case 'down':
      return vertical(layout, current, 1);
  }
}

/**
 * Where focus should land when the focused item disappears.
 *
 * This happens constantly and is not an edge case: filter groups empty out when the asset tab
 * changes, "Reset filters" only exists while filters are modified, "Load more" comes and goes
 * with the loading state. Falling back to the first item every time would fling the user to the
 * top of the page; staying in the same neighbourhood is what makes those transitions unnoticeable.
 */
export function nearest(layout: Layout, lost: GridItem): string | null {
  const survivors = itemsIn(layout, lost.section);
  if (survivors.length === 0) return firstFocusable(layout);

  let best: GridItem | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const item of survivors) {
    // Row dominates: a control on the same row is a better recovery than one directly above it.
    const distance = Math.abs(item.row - lost.row) * 1000 + Math.abs(item.col - lost.col);
    if (distance < bestDistance) {
      bestDistance = distance;
      best = item;
    }
  }
  return best?.id ?? null;
}

/**
 * Row/column for the nth child of a wrapping grid, given its measured column count.
 *
 * The column count cannot come from React state — `repeat(auto-fill, minmax(9.5rem, 1fr))`
 * resolves against the window width, and the asset grid changes its `minmax` per tab as well. It
 * has to be measured from the DOM and passed in here.
 */
export function flowPosition(index: number, columns: number): { row: number; col: number } {
  const safe = Math.max(1, Math.floor(columns));
  return { row: Math.floor(index / safe), col: index % safe };
}
