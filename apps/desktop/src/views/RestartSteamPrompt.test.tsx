/**
 * The restart offer.
 *
 * Two things here are contracts rather than copy, and both are invisible when broken.
 *
 * The **wording** is the whole feature: this dialog exists because a Steam that is running but
 * unreachable explains none of its own symptoms, so the sentence naming the cause is the thing
 * being shipped. Reworded into vagueness it would still render, still dismiss, and still be
 * useless.
 *
 * The **focus seed** is the other. `autoFocus` draws a ring on Cancel either way, so a dialog whose
 * model never took a selection looks correct until the first D-pad press moves nothing — the exact
 * failure `ConfirmReset` and `GameSearchModal` were fixed for. Asserting the model's own `.focused`
 * class rather than `document.activeElement` is what tells the two apart.
 */
import { describe, expect, test } from 'bun:test';
import { cleanup, render, screen } from '@testing-library/react';
import { FocusProvider } from '../focus';
import { RestartSteamPrompt } from './RestartSteamPrompt';

function show() {
  return render(
    <FocusProvider>
      <RestartSteamPrompt onClose={() => {}} onStatus={() => {}} />
    </FocusProvider>,
  );
}

describe('the restart offer', () => {
  test('says why Steam has to be restarted, not merely that it should be', () => {
    show();
    // Normalised, because the paragraph wraps and the DOM keeps the newlines.
    const text = (document.body.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain(
      'Steam must be restarted in order to properly talk to its API and pull your full game list.',
    );
    // The consequence of pressing the button, which the user cannot get back if it surprises them.
    expect(text).toContain('restarting Steam will end it');
    cleanup();
  });

  test('offers a way to stop being asked', () => {
    // Without this the dialog is startup furniture for anyone who cannot or will not restart —
    // the thing this project has deleted a screen for before.
    show();
    expect(screen.getByLabelText(/don.t ask again/i)).toBeTruthy();
    cleanup();
  });

  test('seeds the cursor on the safe button, not the one with the side effect', async () => {
    show();
    // The model's own class, not `document.activeElement`: `autoFocus` sets the latter during
    // commit whether or not the focus model ever learned about it.
    await new Promise((resolve) => queueMicrotask(() => resolve(null)));
    const focused = document.querySelector('.focused');
    expect(focused?.textContent).toBe('Not now');
    cleanup();
  });
});
