/**
 * The focus provider: the registry, the cursor, the scope stack, and the two input listeners.
 *
 * **Two contexts, not one, and the split is load-bearing.**
 *
 * `FocusCtx` carries the registration API and never changes identity. `FocusedIdCtx` carries the
 * cursor and changes on every move. They were one context, with `focusedId` inside it — which
 * meant the context value was a new object after every arrow press, and every hook that registers
 * a control lists the context in its effect dependencies. One press therefore unregistered and
 * re-registered *every* focusable on screen, up to ~250 in a full asset grid, each bumping
 * `revision` and forcing the layout to be rebuilt.
 *
 * Nothing looked wrong, which is why it survived: the cursor moved correctly the whole time.
 *
 * Anything a registration effect depends on belongs in `FocusCtx` and must be a stable
 * `useCallback`. Anything that changes as the cursor moves belongs in `FocusedIdCtx`.
 */
import {
  createContext,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { listen } from '@tauri-apps/api/event';
import { firstFocusable, move, nearest, type GridItem, type Layout } from '@griddle/shared';
import {
  buildLayout,
  isTextEntry,
  KEY_DIRECTIONS,
  openContextMenu,
  type Entry,
  type NavAction,
  type Scope,
  type Screen,
} from './model';

/** Everything a control needs to join the model. Stable for the provider's whole lifetime. */
export interface FocusApi {
  register: (entry: Entry) => void;
  unregister: (id: string) => void;
  setColumns: (section: string, columns: number) => void;
  pushScope: (token: string, name: string, back: () => void) => void;
  popScope: (token: string) => void;
  registerScreen: (screen: Screen) => void;
  unregisterScreen: (token: string) => void;
}

export const FocusCtx = createContext<FocusApi | null>(null);
/** The cursor, separately, so moving it does not change [`FocusCtx`]. */
export const FocusedIdCtx = createContext<string | null>(null);
export const ScopeCtx = createContext<string>('root');

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
    return buildLayout(entries.current.values(), activeScope, columns.current);
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
   * Without this the first directional press inside an overlay is spent *entering* it and
   * appears to do nothing — and the appearance is what makes it a bug rather than a quirk:
   * `autoFocus` has already drawn a `:focus-visible` ring on a button, so the user sees a
   * selection, presses right, and watches the ring stay exactly where it was.
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

  // Mirrors of the state `dispatch` reads. They exist so `dispatch` can be a *stable* callback:
  // it is handed to a Tauri event subscription, and a new identity on every focus change would
  // tear that subscription down and rebuild it several times a second.
  const scopesRef = useRef<Scope[]>([]);
  scopesRef.current = scopes;
  const layoutRef = useRef<Layout>(layout);
  layoutRef.current = layout;

  // Screens live in a ref rather than state: nothing renders differently because one registered,
  // and a re-render per mounted screen would be pure waste.
  const screensRef = useRef<Screen[]>([]);
  const deepestScreen = useCallback(
    () =>
      screensRef.current.reduce<Screen | null>(
        (best, s) => (!best || s.depth > best.depth ? s : best),
        null,
      ),
    [],
  );

  const registerScreen = useCallback((screen: Screen) => {
    screensRef.current = [...screensRef.current.filter((s) => s.token !== screen.token), screen];
  }, []);

  const unregisterScreen = useCallback((token: string) => {
    screensRef.current = screensRef.current.filter((s) => s.token !== token);
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

  /**
   * One place both input sources arrive at.
   *
   * The keyboard handler below and the controller listener further down both funnel into this,
   * so a pad is a second *source* for navigation rather than a second implementation of it.
   *
   * Returns whether anything handled the action, so the keyboard handler can decide whether to
   * swallow the key.
   */
  const dispatch = useCallback(
    (action: NavAction): boolean => {
      if (action === 'back') {
        // Dialogs first, innermost outwards, then the screen itself. B is one button and means
        // "undo the last thing that took me somewhere", whatever that was.
        const top = scopesRef.current[scopesRef.current.length - 1];
        if (top) {
          top.back();
          return true;
        }
        const onBack = deepestScreen()?.actions.current.onBack;
        onBack?.();
        return onBack !== undefined;
      }
      if (action === 'tabPrev' || action === 'tabNext') {
        // Answered by the deepest screen that *has* tabs, so the bumpers switch asset tabs inside
        // a game and the library scope on the list, without either having to know about the other.
        for (const screen of screensRef.current.slice().sort((a, b) => b.depth - a.depth)) {
          const handler =
            action === 'tabPrev'
              ? screen.actions.current.onTabPrev
              : screen.actions.current.onTabNext;
          if (handler) {
            handler();
            return true;
          }
        }
        return false;
      }
      if (action === 'accept') {
        // A real click from the element itself. That matters for the context menu, whose
        // capture-phase dismiss listener unmounts any item whose activation came from outside it.
        if (focusedIdRef.current === null) return false;
        entries.current.get(focusedIdRef.current)?.el.click();
        return true;
      }
      if (action === 'menu') {
        const el = entries.current.get(focusedIdRef.current ?? '')?.el;
        openContextMenu(el);
        return el !== undefined;
      }
      const next = move(layoutRef.current, focusedIdRef.current, action);
      if (next) focusTo(next, true);
      return next !== null;
    },
    [focusTo, deepestScreen],
  );

  /**
   * Controller actions, read natively in Rust and delivered as a Tauri event.
   *
   * Not `navigator.getGamepads()`. WebView2 has two open bugs there, and
   * [#5507](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5507) is the one that
   * matters here: gamepad input dies in WebView2 whenever the Steam Overlay is attached, which is
   * always true of Griddle launched from Big Picture.
   */
  useEffect(() => {
    const unavailable = (err: unknown) => {
      console.error(
        'controller navigation is unavailable: the "nav" subscription was refused.',
        'Check crates/griddle-app/capabilities/default.json grants core:event:allow-listen.',
        err,
      );
    };

    // Both failure shapes are handled, and neither is boilerplate.
    //
    // `listen` is a **core plugin command**, so Tauri v2's capability system can refuse it — and
    // it did: with no capability file at all, every subscription failed with *"event.listen not
    // allowed"*. The promise was never awaited, so the rejection surfaced nowhere and the symptom
    // was a controller that did nothing with no error anywhere. See `capabilities/default.json`.
    //
    // And `listen` can throw **synchronously**, which `.catch()` cannot help with: it reads
    // `window.__TAURI_INTERNALS__` before its first await, so outside a Tauri webview it throws
    // rather than rejecting, and the throw escapes the effect and takes the React tree with it.
    // Always true in the shipped app, never true under test — which is how this was found.
    let pending: Promise<() => void> | null = null;
    try {
      pending = listen<NavAction>('nav', (e) => {
        dispatch(e.payload);
      });
      pending.catch(unavailable);
    } catch (err) {
      unavailable(err);
    }

    return () => {
      void pending?.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [dispatch]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;

      if (e.key === 'Escape') {
        // Same path as the pad's B: the topmost overlay if there is one, otherwise the screen's
        // own way out. Before the scope stack existed, two independent window-level Escape
        // handlers meant one press dismissed the preview *and* the context menu.
        //
        // Prevented only when something actually handled it. Escape used to be swallowed on
        // every screen including the library root, where nothing answers — which silently took
        // the key away from the browser and from anything else that might want it.
        if (dispatch('back')) e.preventDefault();
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
      dispatch(direction);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [tabOrder, focusedId, scopes.length, focusTo, dispatch]);

  // Every member is a `useCallback` with an empty or stable dependency list, so this object is
  // built once and never again. That is the whole point of the split — see the module docs.
  const api = useMemo<FocusApi>(
    () => ({
      register,
      unregister,
      setColumns,
      pushScope,
      popScope,
      registerScreen,
      unregisterScreen,
    }),
    [
      register,
      unregister,
      setColumns,
      pushScope,
      popScope,
      registerScreen,
      unregisterScreen,
    ],
  );

  return (
    <FocusCtx.Provider value={api}>
      <FocusedIdCtx.Provider value={focusedId}>{children}</FocusedIdCtx.Provider>
    </FocusCtx.Provider>
  );
}
