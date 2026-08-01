/**
 * The registration model, tested where it actually lives: React's rendering.
 *
 * The bug these guard against was invisible in use — the cursor moved correctly the whole time,
 * it just re-registered every control on screen to do it. Nothing about the behaviour said so,
 * which is why the guard has to count registrations rather than check appearances.
 */
import { describe, expect, mock, test } from 'bun:test';
import { act, cleanup, render } from '@testing-library/react';
import { useContext, useEffect } from 'react';
import { FocusCtx, FocusProvider } from './provider';
import { useFocusItem, useScreenActions } from './hooks';
import { SCREEN_DEPTH } from './model';

/**
 * Presses a key on `window`, where the provider listens. Returns whether the press was swallowed.
 *
 * 🔴 Read from the event afterwards rather than from a second listener. A probe listener cannot
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
  // 🔴 `ctx` must be in this dependency list, and it is the entire test.
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
    // 🔴 The regression this exists for. `focusedId` used to live in the same context as the
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

describe('Escape', () => {
  /** A screen whose Back handler is optional, which is the whole variable under test. */
  function Screen({ onBack }: { onBack?: () => void }) {
    useScreenActions(SCREEN_DEPTH.app, onBack ? { onBack } : {});
    return null;
  }

  test('is swallowed only when a screen actually handles it', () => {
    // 🔴 Escape used to be prevented on every screen, including the library root where nothing
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
