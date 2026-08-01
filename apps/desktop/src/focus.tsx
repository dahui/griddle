/**
 * The DOM half of spatial focus navigation. The arithmetic lives in `@griddle/shared/focusgrid`.
 *
 * Three jobs this does that the pure module cannot:
 *
 * 1. **Measure wrapping grids.** `repeat(auto-fill, minmax(9.5rem, 1fr))` resolves against the
 *    window, and the asset grid changes its `minmax` per tab as well, so the column count is only
 *    knowable from the laid-out DOM. Children are grouped by `offsetTop`.
 * 2. **Order sections by document position**, rather than making every call site pass an index
 *    that would then have to be kept in step by hand.
 * 3. **Move real DOM focus**, not just a highlight. `el.focus()` means Enter, Space and typing
 *    keep working natively and assistive technology follows along — this layer only decides
 *    *which* element is focused, never what activating it does.
 *
 * 🔴 **Enter and Space are deliberately not intercepted.** Because focus is real, the browser
 * already fires `click` on a focused `<button>` for both. Handling them here too would fire every
 * action twice, and that bug looks like "the app applied the artwork, then applied it again".
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import {
  firstFocusable,
  flowPosition,
  move,
  nearest,
  type Direction,
  type GridItem,
  type Layout,
} from '@griddle/shared';

/** Where an item sits: fixed for bars and stacks, flow for a wrapping grid. */
type Placement = { kind: 'fixed'; row: number; col: number } | { kind: 'flow'; index: number };

interface Entry {
  id: string;
  scope: string;
  section: string;
  placement: Placement;
  el: HTMLElement;
}

interface Scope {
  token: string;
  name: string;
  back: () => void;
  /** What was focused when this scope opened, so closing it can put focus back. */
  restore: string | null;
}

interface FocusApi {
  focusedId: string | null;
  register: (entry: Entry) => void;
  unregister: (id: string) => void;
  setColumns: (section: string, columns: number) => void;
  pushScope: (token: string, name: string, back: () => void) => void;
  popScope: (token: string) => void;
  /** Activate the focused control. Unused by the keyboard, which activates natively. */
  activate: () => void;
}

const FocusCtx = createContext<FocusApi | null>(null);
const ScopeCtx = createContext<string>('root');

const KEY_DIRECTIONS: Record<string, Direction> = {
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
function isTextEntry(el: Element | null): boolean {
  if (!(el instanceof HTMLInputElement)) return el instanceof HTMLTextAreaElement;
  return ['text', 'search', 'password', 'email', 'url', 'number'].includes(el.type);
}

function documentOrder(a: HTMLElement, b: HTMLElement): number {
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
function measureColumns(container: HTMLElement): number {
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

export function FocusProvider({ children }: { children: ReactNode }) {
  const entries = useRef(new Map<string, Entry>());
  const columns = useRef(new Map<string, number>());
  // Bumped whenever the registry or a measurement changes, to recompute the layout. The registry
  // itself is a ref: it is written from a dozen child effects per render, and holding it in state
  // would mean a render per control.
  const [revision, setRevision] = useState(0);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const [scopes, setScopes] = useState<Scope[]>([]);

  // Mirrored into a ref so `pushScope` can capture what was focused without closing over the
  // state, which would make it a new function on every keystroke and re-run every scope effect.
  const focusedIdRef = useRef<string | null>(null);
  focusedIdRef.current = focusedId;

  const activeScope = scopes[scopes.length - 1]?.name ?? 'root';

  const register = useCallback((entry: Entry) => {
    entries.current.set(entry.id, entry);
    setRevision((r) => r + 1);
  }, []);

  const unregister = useCallback((id: string) => {
    entries.current.delete(id);
    setRevision((r) => r + 1);
  }, []);

  const setColumns = useCallback((section: string, count: number) => {
    // Guarded, because this is called from a ResizeObserver: an unconditional bump would
    // re-render, re-measure and bump again.
    if (columns.current.get(section) === count) return;
    columns.current.set(section, count);
    setRevision((r) => r + 1);
  }, []);

  /** The scoped, ordered view of the registry that navigation runs against. */
  const { layout, tabOrder } = useMemo(() => {
    void revision;
    const inScope = [...entries.current.values()].filter((e) => e.scope === activeScope);
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
      const position = flowPosition(entry.placement.index, columns.current.get(entry.section) ?? 1);
      return { id: entry.id, section: entry.section, ...position };
    });

    return {
      layout: { items, sections, flow: [...flow] } satisfies Layout,
      tabOrder: inScope.map((e) => e.id),
    };
  }, [revision, activeScope]);

  const focusTo = useCallback((id: string, scroll: boolean) => {
    setFocusedId(id);
    const el = entries.current.get(id)?.el;
    if (!el) return;
    // `preventScroll` then an explicit `scrollIntoView`, so the scroll is ours to control: the
    // browser's own focus scroll centres the element, which lurches the page on every step.
    el.focus({ preventScroll: true });
    if (scroll) el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, []);

  // The last known position of the focused item, kept so that when it disappears we can recover
  // to its neighbourhood rather than to the top of the page.
  const lastPosition = useRef<GridItem | null>(null);
  useEffect(() => {
    const current = layout.items.find((i) => i.id === focusedId);
    if (current) {
      lastPosition.current = current;
      return;
    }
    if (focusedId === null || !lastPosition.current) return;
    const recovered = nearest(layout, lastPosition.current);
    // No scroll: the user did not ask to move, the ground moved under them.
    if (recovered) focusTo(recovered, false);
    else setFocusedId(null);
  }, [layout, focusedId, focusTo]);

  const pushScope = useCallback((token: string, name: string, back: () => void) => {
    setScopes((s) => [...s, { token, name, back, restore: focusedIdRef.current }]);
    // Clear the selection: it belongs to the screen behind the overlay. The effect below picks
    // a new one as soon as the overlay's controls have registered.
    setFocusedId(null);
  }, []);

  /**
   * Give a freshly opened overlay a selection immediately.
   *
   * 🔴 Without this the first directional press inside an overlay is spent *entering* it and
   * appears to do nothing — and the appearance is what makes it a bug rather than a quirk:
   * `autoFocus` has already drawn a `:focus-visible` ring on a button, so the user sees a
   * selection, presses right, and watches the ring stay exactly where it was. Caught by driving
   * the reset dialog from the keyboard and comparing two screenshots that should have differed.
   *
   * Whatever the overlay chose to `autoFocus` wins, because those choices are deliberate:
   * `GameSearchModal` puts the caret in its search box so the user can type straight away, and
   * `ConfirmReset` focuses Cancel so the destructive button is never the default.
   */
  useEffect(() => {
    if (scopes.length === 0 || focusedId !== null) return;
    for (const [id, entry] of entries.current) {
      if (entry.scope === activeScope && entry.el === document.activeElement) {
        setFocusedId(id);
        return;
      }
    }
    const first = firstFocusable(layout);
    // No scroll: an overlay is already where the user is looking.
    if (first) focusTo(first, false);
  }, [scopes.length, activeScope, focusedId, layout, focusTo]);

  const popScope = useCallback(
    (token: string) => {
      setScopes((s) => {
        const leaving = s.find((x) => x.token === token);
        const restore = leaving?.restore;
        if (restore) {
          // Deferred, because this runs while the overlay is unmounting: the opener is only
          // laid out again — and only scrollable to — once the backdrop is gone.
          queueMicrotask(() => {
            if (entries.current.has(restore)) focusTo(restore, true);
          });
        }
        return s.filter((x) => x.token !== token);
      });
    },
    [focusTo],
  );

  const activate = useCallback(() => {
    if (focusedIdRef.current === null) return;
    entries.current.get(focusedIdRef.current)?.el.click();
  }, []);

  // Keep the model in step when focus arrives by mouse or Tab, so a directional press afterwards
  // continues from where the user actually is.
  useEffect(() => {
    const onFocusIn = (e: FocusEvent) => {
      const target = e.target;
      if (!(target instanceof HTMLElement)) return;
      for (const [id, entry] of entries.current) {
        if (entry.el === target && entry.scope === activeScope) {
          setFocusedId(id);
          return;
        }
      }
    };
    window.addEventListener('focusin', onFocusIn);
    return () => window.removeEventListener('focusin', onFocusIn);
  }, [activeScope]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;

      if (e.key === 'Escape') {
        const top = scopes[scopes.length - 1];
        // Only the topmost overlay closes. Before this existed, two independent window-level
        // Escape handlers meant one press dismissed the preview *and* the context menu.
        if (top) {
          e.preventDefault();
          top.back();
        }
        return;
      }

      // A real focus trap for overlays, built from the registry we already keep. Without it Tab
      // walks out of a modal into the library behind it.
      if (e.key === 'Tab' && scopes.length > 0 && tabOrder.length > 0) {
        e.preventDefault();
        const at = focusedId ? tabOrder.indexOf(focusedId) : -1;
        const step = e.shiftKey ? -1 : 1;
        const next = tabOrder[(at + step + tabOrder.length) % tabOrder.length];
        if (next) focusTo(next, true);
        return;
      }

      const direction = KEY_DIRECTIONS[e.key];
      if (!direction) return;
      if (isTextEntry(document.activeElement) && (direction === 'left' || direction === 'right')) {
        return;
      }
      // Always prevented, even when there is nowhere to go: otherwise running into the edge of a
      // section scrolls the page instead, which reads as focus having been lost.
      e.preventDefault();
      const next = move(layout, focusedId, direction);
      if (next) focusTo(next, true);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [layout, tabOrder, focusedId, scopes, focusTo]);

  const api = useMemo<FocusApi>(
    () => ({ focusedId, register, unregister, setColumns, pushScope, popScope, activate }),
    [focusedId, register, unregister, setColumns, pushScope, popScope, activate],
  );

  return <FocusCtx.Provider value={api}>{children}</FocusCtx.Provider>;
}

/**
 * Register one control at a fixed spot in its section.
 *
 * `row`/`col` are the control's position *within its section*, not on the page — a two-button bar
 * is row 0, columns 0 and 1, regardless of what sits above it.
 */
export function useFocusItem<T extends HTMLElement = HTMLElement>(
  section: string,
  row: number,
  col: number,
) {
  const ref = useRef<T | null>(null);
  const id = useId();
  const ctx = useContext(FocusCtx);
  const scope = useContext(ScopeCtx);

  useEffect(() => {
    const el = ref.current;
    if (!el || !ctx) return undefined;
    ctx.register({ id, scope, section, placement: { kind: 'fixed', row, col }, el });
    return () => ctx.unregister(id);
  }, [ctx, id, scope, section, row, col]);

  return { ref, focused: ctx?.focusedId === id };
}

/**
 * Register one tile of a wrapping grid by its index; row and column are derived from the measured
 * column count. Pair with [`useFocusGrid`] on the container.
 */
export function useFocusGridItem<T extends HTMLElement = HTMLElement>(
  section: string,
  index: number,
) {
  const ref = useRef<T | null>(null);
  const id = useId();
  const ctx = useContext(FocusCtx);
  const scope = useContext(ScopeCtx);

  useEffect(() => {
    const el = ref.current;
    if (!el || !ctx) return undefined;
    ctx.register({ id, scope, section, placement: { kind: 'flow', index }, el });
    return () => ctx.unregister(id);
  }, [ctx, id, scope, section, index]);

  return { ref, focused: ctx?.focusedId === id };
}

/**
 * Attach to a wrapping grid container so its column count stays measured.
 *
 * Both observers earn their place: `ResizeObserver` catches the window being resized, and
 * `MutationObserver` catches the grid growing from infinite scroll **and** the asset tab changing
 * — that one alters `minmax` without altering the container's width, so a resize observer alone
 * would silently keep the previous tab's column count.
 */
export function useFocusGrid<T extends HTMLElement = HTMLElement>(section: string) {
  const ref = useRef<T | null>(null);
  const ctx = useContext(FocusCtx);

  useEffect(() => {
    const el = ref.current;
    if (!el || !ctx) return undefined;
    const measure = () => ctx.setColumns(section, measureColumns(el));
    measure();
    const resize = new ResizeObserver(measure);
    resize.observe(el);
    const mutation = new MutationObserver(measure);
    mutation.observe(el, { childList: true });
    return () => {
      resize.disconnect();
      mutation.disconnect();
    };
  }, [ctx, section]);

  return ref;
}

/**
 * An overlay's own navigation scope.
 *
 * While one is mounted, only the controls inside it are reachable, Escape closes **it** rather
 * than everything at once, and closing returns focus to whatever opened it. All three were
 * missing: there was no focus restoration anywhere in the app, and two overlays hand-rolled a
 * window-level Escape listener each while the other two had none at all.
 */
export function FocusScope({
  name,
  onBack,
  children,
}: {
  name: string;
  onBack: () => void;
  children: ReactNode;
}) {
  const token = useId();
  const ctx = useContext(FocusCtx);
  // Held in a ref so a caller passing an inline arrow does not tear the scope down and rebuild it
  // on every render — which would lose the captured focus and re-enter the overlay each time.
  const back = useRef(onBack);
  back.current = onBack;

  useEffect(() => {
    if (!ctx) return undefined;
    ctx.pushScope(token, name, () => back.current());
    return () => ctx.popScope(token);
  }, [ctx, token, name]);

  return <ScopeCtx.Provider value={name}>{children}</ScopeCtx.Provider>;
}

/** Activate the focused control programmatically. For the gamepad's Accept; the keyboard does not need it. */
export function useActivate() {
  return useContext(FocusCtx)?.activate ?? (() => {});
}
