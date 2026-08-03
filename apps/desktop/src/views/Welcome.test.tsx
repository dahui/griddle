/**
 * The first-run screen.
 *
 * The interesting assertion is the focus one. Everything else here is copy, and copy that is
 * wrong is visible the moment anybody looks at the screen — but a cursor that never got seeded
 * looks *fine*: the caret blinks in the field, and only the first D-pad press reveals that the
 * model does not know where it is.
 */
import { describe, expect, test } from 'bun:test';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { FocusProvider } from '../focus';
import { Welcome } from './Welcome';
import type { Status } from '../api';

function statusWith(over: Partial<Status> = {}): Status {
  return {
    app_version: '0.0.0',
    steam_root: 'C:\\Program Files (x86)\\Steam',
    steam_running: true,
    offer_to_start_steam: true,
    steam_source: 'HKCU',
    account_id: 1,
    has_api_key: false,
    key_unreadable: false,
    sentinel_explanation: 'Live apply is on.',
    app_types_loaded: 2930,
    steam_error: null,
    ...over,
  };
}

function show(status: Status) {
  return render(
    <FocusProvider>
      <Welcome status={status} onStatus={() => undefined} />
    </FocusProvider>,
  );
}

describe('Welcome', () => {
  test('seeds the focus model, not just the DOM, on the key field', async () => {
    // `.focused` is rendered from the model's cursor (`focusedId === id`), so asserting it
    // proves the provider learned about the focus -- which is the part React's `autoFocus`
    // attribute cannot deliver. `autoFocus` runs during commit, before the control's
    // registration effect, so the provider's `focusin` lookup finds nothing and the cursor stays
    // null. Rewriting KeyInput to use plain `autoFocus` must fail this test.
    show(statusWith());
    const field = await screen.findByPlaceholderText('Paste your API key');
    expect(document.activeElement).toBe(field);
    expect(field.className).toContain('focused');
    cleanup();
  });

  test('leads with what the app is, and how to get a key', () => {
    show(statusWith());
    // Just "Welcome": the lockup beside it carries the name, and the app header shows it too.
    expect(screen.getByRole('heading').textContent).toContain('Welcome');
    // The steps are an ordered list on purpose: they are instructions, and a screen reader
    // should say so.
    expect(screen.getByRole('list').tagName).toBe('OL');
    expect(screen.getAllByRole('listitem')).toHaveLength(4);
    expect(screen.getByText('Open SteamGridDB')).toBeDefined();
    cleanup();
  });

  test('an unreadable stored key is explained, not re-welcomed', () => {
    // This user set Griddle up months ago on another PC. "Welcome, here is what this app does"
    // would be a strange thing to tell them, and would not say what went wrong.
    show(statusWith({ has_api_key: true, key_unreadable: true }));
    expect(screen.getByRole('heading').textContent).toContain('Enter your API key again');
    expect(document.body.textContent).toContain('could not read it');
    cleanup();
  });

  test('a missing Steam is stated rather than left to be discovered', () => {
    show(statusWith({ steam_error: 'no SteamPath value', steam_root: null }));
    expect(document.body.textContent).toContain('Steam was not found');
    // Not a blocker: the key is still worth saving.
    expect(screen.getByPlaceholderText('Paste your API key')).toBeDefined();
    cleanup();
  });

  test('says nothing about Steam when Steam is fine', () => {
    show(statusWith());
    expect(document.body.textContent).not.toContain('Steam was not found');
    cleanup();
  });

  describe('the paste hint', () => {
    /** Obviously synthetic: 32 hex characters, the shape SteamGridDB issues. */
    const WELL_FORMED = '0123456789abcdef0123456789abcdef';

    function type(value: string) {
      show(statusWith());
      const field = screen.getByPlaceholderText('Paste your API key');
      fireEvent.change(field, { target: { value } });
      return screen.getByRole('button', { name: /Save/ }) as HTMLButtonElement;
    }

    test('warns about a truncated paste but leaves Save usable', () => {
      // The warning must never become a gate. `ApiKey::new` accepts any shape on purpose --
      // today's 32-hex form is observed, not promised -- so blocking here would enforce a rule
      // Rust deliberately declined to enforce, and would brick the app if the format ever moved.
      const save = type(WELL_FORMED.slice(0, 20));
      expect(document.body.textContent).toContain('doesn’t look like a SteamGridDB key');
      expect(save.disabled).toBe(false);
      cleanup();
    });

    test('stays quiet for a well-formed key, and for one pasted with its Bearer label', () => {
      for (const value of [WELL_FORMED, `Bearer ${WELL_FORMED}`]) {
        type(value);
        expect(document.body.textContent).not.toContain('doesn’t look like');
        cleanup();
      }
    });

    test('stays quiet on an empty field', () => {
      // Nothing typed is not a mistake worth remarking on.
      const save = type('');
      expect(document.body.textContent).not.toContain('doesn’t look like');
      // ...and there is nothing to save yet either.
      expect(save.disabled).toBe(true);
      cleanup();
    });
  });
});
