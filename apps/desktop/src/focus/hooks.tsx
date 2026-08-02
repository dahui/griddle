/**
 * What a view calls to join the focus model.
 *
 * Each registration effect depends on `FocusCtx`, which never changes identity — so a control
 * registers when it mounts and not again. Being focused is read from `FocusedIdCtx` separately;
 * see [`./provider`] for why those are two contexts.
 */
import { useContext, useEffect, useId, useRef, type ReactNode } from 'react';
import { FocusCtx, FocusedIdCtx, ScopeCtx } from './provider';
import { measureColumns, type ScreenActions } from './model';

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
  const focusedId = useContext(FocusedIdCtx);
  const scope = useContext(ScopeCtx);

  useEffect(() => {
    const el = ref.current;
    if (!el || !ctx) return undefined;
    ctx.register({ id, scope, section, placement: { kind: 'fixed', row, col }, el });
    return () => ctx.unregister(id);
  }, [ctx, id, scope, section, row, col]);

  return { ref, focused: focusedId === id };
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
  const focusedId = useContext(FocusedIdCtx);
  const scope = useContext(ScopeCtx);

  useEffect(() => {
    const el = ref.current;
    if (!el || !ctx) return undefined;
    ctx.register({ id, scope, section, placement: { kind: 'flow', index }, el });
    return () => ctx.unregister(id);
  }, [ctx, id, scope, section, index]);

  return { ref, focused: focusedId === id };
}

/**
 * Attach to a wrapping grid container so its column count stays measured.
 *
 * All three triggers earn their place, and each covers a case the others cannot see:
 *
 * - `ResizeObserver` — the window being resized.
 * - `MutationObserver` on `childList` — the grid growing from infinite scroll.
 * - `MutationObserver` on `style` — the **asset tab changing** and the **zoom being stepped**.
 *   Both re-layout the same children inside a container of the same width, so neither of the
 *   first two fires at all.
 *
 * That last one is the whole hazard. A stale column count is not a visible failure: the tiles
 * render perfectly and only *navigation* is wrong, so pressing down moves two rows or lands in a
 * different column, which reads as the focus model being broken rather than as a measurement
 * that was never retaken.
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
    // `setColumns` ignores a count it already holds, so the extra callbacks this brings in cost
    // a `measureColumns` and stop there — no render, no observer feedback loop.
    mutation.observe(el, { childList: true, attributes: true, attributeFilter: ['style', 'class'] });
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

/**
 * Claim the buttons that belong to a screen rather than to a control: B, and the bumpers.
 *
 * `depth` decides who answers when several screens are mounted — see `SCREEN_DEPTH`. Omit a
 * handler and the next screen out gets that button, which is what lets the bumpers fall through
 * to the Library/Settings switch on a screen that has no tabs of its own.
 */
export function useScreenActions(depth: number, actions: ScreenActions) {
  const token = useId();
  const ctx = useContext(FocusCtx);
  // Held in a ref and refreshed every render, so callers can pass inline closures over current
  // state without re-registering — and so a handler never fires against a stale snapshot.
  const latest = useRef(actions);
  latest.current = actions;

  useEffect(() => {
    if (!ctx) return undefined;
    ctx.registerScreen({ token, depth, actions: latest });
    return () => ctx.unregisterScreen(token);
  }, [ctx, token, depth]);
}
