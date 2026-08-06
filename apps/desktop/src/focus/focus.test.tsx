/**
 * The registration model, tested where it actually lives: React's rendering.
 *
 * The bug these guard against was invisible in use — the cursor moved correctly the whole time,
 * it just re-registered every control on screen to do it. Nothing about the behaviour said so,
 * which is why the guard has to count registrations rather than check appearances.
 */
import { describe, expect, mock, test } from 'bun:test';
import { act, cleanup, render } from '@testing-library/react';
import { useContext, useEffect, useMemo, useRef } from 'react';
import { FocusCtx, FocusProvider } from './provider';
import { useFocusGrid, useFocusItem, useScreenActions } from './hooks';
import { SCREEN_DEPTH } from './model';

/**
 * Presses a key on `window`, where the provider listens. Returns whether the press was swallowed.
 *
 * Read from the event afterwards rather than from a second listener. A probe listener cannot
 * answer this: React runs child effects before parent effects, so a probe registered by any child
 * of the provider is added to `window` *first* and therefore fires *before* the provider has
 * decided anything. It reported "not prevented" every time, whatever the provider did.
 */
function press(key: string): boolean {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  act(() => {
    window.dispatchEvent(event);
  });
  return event.defaultPrevented;
}

/**
 * A row of buttons that reports every time one of them registers.
 *
 * The count is what the assertions read: `onRegister` fires from the same effect that calls
 * `ctx.register`, so it tracks registrations one-for-one.
 */
function Row({ count, onRegister }: { count: number; onRegister: () => void }) {
  return (
    <>
      {Array.from({ length: count }, (_, i) => (
        <Cell key={i} col={i} onRegister={onRegister} />
      ))}
    </>
  );
}

function Cell({ col, onRegister }: { col: number; onRegister: () => void }) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('row', 0, col);
  // `ctx` must be in this dependency list, and it is the entire test.
  //
  // `useFocusItem`'s own registration effect depends on the context object, so this effect fires
  // exactly when that one does. An earlier version depended only on `onRegister` -- a stable mock
  // -- so it ran once no matter what the provider did, and passed just as happily against the
  // buggy single-context provider. It measured nothing.
  const ctx = useContext(FocusCtx);
  useEffect(onRegister, [ctx, onRegister]);
  return (
    <button ref={ref} type="button" data-testid={`cell-${col}`} className={focused ? 'focused' : ''}>
      {col}
    </button>
  );
}

describe('focus registration', () => {
  test('moving the cursor does not re-register the controls', () => {
    // The regression this exists for. `focusedId` used to live in the same context as the
    // registration API, so the context value was a new object after every arrow press and every
    // registration effect re-ran -- up to ~250 controls in a full asset grid, each unregistering
    // and re-registering and bumping the layout revision.
    //
    // Verified to fail against the single-context version by reintroducing it: 12 registrations
    // for 4 controls and 2 presses, where 4 is correct.
    const onRegister = mock(() => {});
    render(
      <FocusProvider>
        <Row count={4} onRegister={onRegister} />
      </FocusProvider>,
    );

    const afterMount = onRegister.mock.calls.length;
    expect(afterMount).toBe(4);

    press('ArrowRight');
    press('ArrowRight');

    expect(onRegister.mock.calls.length).toBe(afterMount);
    cleanup();
  });

  test('the cursor still moves, so the test above cannot pass by doing nothing', () => {
    // The control. Without it, a provider that ignored every key press and registered nothing
    // twice would satisfy the assertion above perfectly.
    render(
      <FocusProvider>
        <Row count={3} onRegister={() => {}} />
      </FocusProvider>,
    );

    // Nothing is focused until the first press lands, so the model starts from the first item.
    press('ArrowRight');
    const first = document.querySelector('.focused');
    expect(first?.getAttribute('data-testid')).toBe('cell-0');

    press('ArrowRight');
    expect(document.querySelector('.focused')?.getAttribute('data-testid')).toBe('cell-1');
    cleanup();
  });
});

describe('grid column measurement', () => {
  /** Exposes the container so a test can mutate it the way the zoom control does. */
  function Grid() {
    const ref = useFocusGrid<HTMLDivElement>('assets');
    return <div ref={ref} data-testid="grid" />;
  }

  /**
   * Capture `setColumns` calls without replacing the provider.
   *
   * The real provider is what wires the observers, so the spy sits between it and the grid rather
   * than standing in for it — a hand-built fake context would test the fake.
   *
   * **The `useMemo` is not tidiness, it is the test.** This built the wrapped object inline, so it
   * had a new identity on every render — which is precisely the instability the real provider is
   * built to avoid, and which `useFocusGrid` used to depend on to recover from a null ref. Against
   * that spy the late-mount test below passed happily on the *broken* hook: the churn re-ran its
   * effect for it. A spy must reproduce the provider's stability guarantee or it hides the class of
   * bug that guarantee exists for.
   */
  function Spy({ onMeasure, children }: { onMeasure: () => void; children: React.ReactNode }) {
    const real = useContext(FocusCtx);
    // Refreshed every render and read through the ref, so the callback can be captured without
    // making the context value depend on it.
    const notify = useRef(onMeasure);
    notify.current = onMeasure;
    const wrapped = useMemo(
      () =>
        real && {
          ...real,
          setColumns: (s: string, n: number) => {
            notify.current();
            real.setColumns(s, n);
          },
        },
      [real],
    );
    if (!wrapped) return null;
    return <FocusCtx.Provider value={wrapped}>{children}</FocusCtx.Provider>;
  }

  test('changing the container style re-measures the column count', async () => {
    // The zoom control's whole mechanism: it writes `--tile` to the container's inline style.
    // That re-flows the same children inside a container of the same width, so neither the
    // ResizeObserver nor a childList MutationObserver sees anything at all.
    //
    // A stale count is invisible — the tiles render perfectly and only *navigation* is wrong, so
    // pressing down moves two rows. Confirmed to fail with `attributes` removed from the observer
    // options: the count stays at its mount-time measurement forever.
    const onMeasure = mock(() => {});
    render(
      <FocusProvider>
        <Spy onMeasure={onMeasure}>
          <Grid />
        </Spy>
      </FocusProvider>,
    );

    const atMount = onMeasure.mock.calls.length;
    expect(atMount).toBeGreaterThan(0);

    const grid = document.querySelector('[data-testid="grid"]') as HTMLElement;
    await act(async () => {
      grid.style.setProperty('--tile', '14rem');
      // MutationObserver callbacks are delivered as a microtask, not synchronously.
      await Promise.resolve();
    });

    expect(onMeasure.mock.calls.length).toBeGreaterThan(atMount);
    cleanup();
  });

  /**
   * The shape every real consumer has, and the one `Grid` above does not.
   *
   * `Library` and `CurrentAssets` both return a `<Spinner>` on their first commit and only render
   * the grid once their data arrives. `Show` reproduces exactly that: nothing, then the container.
   */
  function Show({ visible }: { visible: boolean }) {
    const ref = useFocusGrid<HTMLDivElement>('assets');
    if (!visible) return null;
    return <div ref={ref} data-testid="late-grid" />;
  }

  test('a container that mounts after the first commit is still measured', async () => {
    // The bug: `useFocusGrid` measured from an effect whose dependencies were both permanently
    // stable, so it ran exactly once -- with `ref.current` still null, because the view was
    // showing a spinner. It bailed and was never invoked again, the column count stayed unset,
    // and `buildLayout`'s `?? 1` made every tile its own row. Up and down then stepped one tile
    // sideways, which is how the library grid was reported as broken.
    //
    // Confirmed to fail against the `useRef` + `useEffect` version: zero measurements, ever.
    const onMeasure = mock(() => {});
    const { rerender } = render(
      <FocusProvider>
        <Spy onMeasure={onMeasure}>
          <Show visible={false} />
        </Spy>
      </FocusProvider>,
    );

    expect(onMeasure.mock.calls.length).toBe(0);

    await act(async () => {
      rerender(
        <FocusProvider>
          <Spy onMeasure={onMeasure}>
            <Show visible />
          </Spy>
        </FocusProvider>,
      );
    });

    expect(onMeasure.mock.calls.length).toBeGreaterThan(0);
    cleanup();
  });

  test('a container replaced by a new element is re-measured', async () => {
    // The second half, which survives even a fixed first mount. The library swaps its `<ul>` for
    // a spinner on every scope or sort change, and for an empty state when the filter matches
    // nothing -- so the returning `<ul>` is a *different element*. A ref object never says so,
    // and the observers would stay attached to the detached one.
    const onMeasure = mock(() => {});
    const tree = (visible: boolean) => (
      <FocusProvider>
        <Spy onMeasure={onMeasure}>
          <Show visible={visible} />
        </Spy>
      </FocusProvider>
    );
    const { rerender } = render(tree(true));

    const first = document.querySelector('[data-testid="late-grid"]');
    const afterMount = onMeasure.mock.calls.length;
    expect(afterMount).toBeGreaterThan(0);

    await act(async () => {
      rerender(tree(false));
    });
    await act(async () => {
      rerender(tree(true));
    });

    // A genuinely new node, so the assertion below is about re-attachment and not about a
    // re-render of the same element.
    expect(document.querySelector('[data-testid="late-grid"]')).not.toBe(first);
    expect(onMeasure.mock.calls.length).toBeGreaterThan(afterMount);
    cleanup();
  });
});

describe('Escape', () => {
  /** A screen whose Back handler is optional, which is the whole variable under test. */
  function Screen({ onBack }: { onBack?: () => void }) {
    useScreenActions(SCREEN_DEPTH.app, onBack ? { onBack } : {});
    return null;
  }

  test('is swallowed only when a screen actually handles it', () => {
    // Escape used to be prevented on every screen, including the library root where nothing
    // answers -- silently taking the key from the browser and from anything else that wants it.
    const onBack = mock(() => {});

    render(
      <FocusProvider>
        <Screen onBack={onBack} />
      </FocusProvider>,
    );
    expect(press('Escape')).toBe(true);
    expect(onBack).toHaveBeenCalledTimes(1);
    cleanup();

    // The other direction, and the half that was broken: a screen with no Back handler must
    // leave the key alone. Without this case the test passes against the old always-prevent code.
    render(
      <FocusProvider>
        <Screen />
      </FocusProvider>,
    );
    expect(press('Escape')).toBe(false);
    cleanup();
  });
});
