/**
 * The DOM-facing pieces of focus navigation that are not React: the registry's shapes, the
 * measurements only a laid-out page can answer, and the two input translations.
 *
 * Kept apart from the provider so each can be read on its own — and because none of it needs
 * hooks, none of it can accidentally acquire state.
 */
import { flowPosition, type Direction, type GridItem, type Layout } from '@griddle/shared';

/**
 * The one navigation vocabulary, shared by the keyboard and the controller.
 *
 * Kept identical to `griddle_core::input::Action`'s serde representation — Rust emits these exact
 * strings. Adding a variant means adding it in both places.
 */
export type NavAction = Direction | 'accept' | 'back' | 'menu' | 'tabPrev' | 'tabNext';

/**
 * What a screen can do with the buttons that are not about moving a cursor.
 *
 * Every field is optional: a screen with no tabs simply does not answer for the bumpers, and the
 * next screen out gets them instead.
 */
export interface ScreenActions {
  /** B, once every dialog is dismissed. Leaving the screen, or cancelling what it is doing. */
  onBack?: () => void;
  onTabPrev?: () => void;
  onTabNext?: () => void;
}

/**
 * Which screen answers a button when several are mounted at once.
 *
 * An explicit number rather than "whichever registered last". React runs **child effects before
 * parent effects**, so registration order is inside-out — taking the most recent entry would hand
 * every button to the outermost screen, which is exactly backwards. Depth says what is meant.
 */
export const SCREEN_DEPTH = {
  /** The Library/Settings switch. Answers only when nothing more specific does. */
  app: 0,
  /** The library list: Installed / All games. */
  library: 1,
  /** One game's asset tabs — the innermost screen there is. */
  game: 2,
} as const;

/** Where an item sits: fixed for bars and stacks, flow for a wrapping grid. */
export type Placement =
  | { kind: 'fixed'; row: number; col: number }
  | { kind: 'flow'; index: number };

export interface Entry {
  id: string;
  scope: string;
  section: string;
  placement: Placement;
  el: HTMLElement;
}

export interface Scope {
  token: string;
  name: string;
  back: () => void;
  /** What was focused when this scope opened, so closing it can put focus back. */
  restore: string | null;
}

export interface Screen {
  token: string;
  depth: number;
  actions: { current: ScreenActions };
}

export const KEY_DIRECTIONS: Record<string, Direction> = {
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
};

/**
 * Whether a key should be left alone because the user is typing into something.
 *
 * Only left/right are surrendered: those are cursor movement inside the field, and stealing them
 * would make the search and API-key boxes uneditable. Up/down still navigate away, which is what
 * makes a search box something a controller can leave.
 */
export function isTextEntry(el: Element | null): boolean {
  if (!(el instanceof HTMLInputElement)) return el instanceof HTMLTextAreaElement;
  return ['text', 'search', 'password', 'email', 'url', 'number'].includes(el.type);
}

export function documentOrder(a: HTMLElement, b: HTMLElement): number {
  if (a === b) return 0;
  const relation = a.compareDocumentPosition(b);
  if (relation & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
  if (relation & Node.DOCUMENT_POSITION_PRECEDING) return 1;
  return 0;
}

/**
 * How many columns a wrapping grid currently has.
 *
 * Counts children sharing the first child's `offsetTop`. Reading the computed
 * `grid-template-columns` would also work and is tempting, but it returns resolved pixel tracks
 * that still have to be counted — and it says nothing about a container that is not a grid.
 * Grouping by position is true of any layout that wraps.
 */
export function measureColumns(container: HTMLElement): number {
  const children = Array.from(container.children).filter(
    (c): c is HTMLElement => c instanceof HTMLElement,
  );
  const first = children[0];
  if (!first) return 1;
  let count = 0;
  for (const child of children) {
    if (child.offsetTop !== first.offsetTop) break;
    count++;
  }
  return Math.max(1, count);
}

/**
 * Open a control's context menu from the keyboard or pad, which have no cursor.
 *
 * The menu positions itself from `clientX`/`clientY`, so the anchor is synthesised from the
 * element's own box — bottom-left of it, where a menu opened by clicking there would appear. A
 * plain `new MouseEvent('contextmenu')` carries 0,0 and would pin every menu to the top-left
 * corner of the window.
 */
export function openContextMenu(el: HTMLElement | undefined) {
  if (!el) return;
  const box = el.getBoundingClientRect();
  el.dispatchEvent(
    new MouseEvent('contextmenu', {
      bubbles: true,
      cancelable: true,
      clientX: Math.round(box.left + 8),
      clientY: Math.round(box.bottom - 8),
    }),
  );
}

/**
 * Turn the registry into the scoped, ordered view navigation runs against.
 *
 * Sections are ordered by document position rather than by an index every call site would have to
 * pass and keep in step by hand. Flow sections have their row and column derived here, from the
 * measured column count, so the arithmetic in `@griddle/shared` never has to know about layout.
 */
export function buildLayout(
  entries: Iterable<Entry>,
  activeScope: string,
  columns: Map<string, number>,
): { layout: Layout; tabOrder: string[] } {
  const inScope = [...entries].filter((e) => e.scope === activeScope);
  inScope.sort((a, b) => documentOrder(a.el, b.el));

  const sections: string[] = [];
  for (const entry of inScope) {
    if (!sections.includes(entry.section)) sections.push(entry.section);
  }

  const flow = new Set<string>();
  const items: GridItem[] = inScope.map((entry) => {
    if (entry.placement.kind === 'fixed') {
      return {
        id: entry.id,
        section: entry.section,
        row: entry.placement.row,
        col: entry.placement.col,
      };
    }
    flow.add(entry.section);
    const position = flowPosition(entry.placement.index, columns.get(entry.section) ?? 1);
    return { id: entry.id, section: entry.section, ...position };
  });

  return {
    layout: { items, sections, flow: [...flow] } satisfies Layout,
    tabOrder: inScope.map((e) => e.id),
  };
}
