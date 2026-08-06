/**
 * Offered once at startup, when Steam is running but its debugging port is not.
 *
 * That state is invisible and it is one Griddle creates. `.cef-enable-remote-debugging` is written
 * silently at every launch, and Steam reads it only when it *starts* — so on the launch that first
 * creates it, a Steam that is already up has no port. The user then gets artwork that only appears
 * at the next restart and an **All games** list a few hundred games short, with nothing anywhere
 * connecting either symptom to a flag they never saw created.
 *
 * The sibling of [`./StartSteamPrompt`] and structurally identical to it, but a separate question
 * with a separate off switch: that one asks to *start* a program, this one asks to *stop* one and
 * takes any running game with it. Somebody may well want the first and not the second.
 *
 * Cancel is focused first and the action is second, as everywhere else in this app: restarting
 * Steam is a side effect, and a reflexive Enter or A press must not be what triggers it.
 */
import { useState } from 'react';
import { api, asUiError, type Status, type UiError } from '../api';
import { ErrorNote, FocusButton, useToast } from '../components';
import { FocusScope, useFocusItem } from '../focus';

export function RestartSteamPrompt({
  onClose,
  onStatus,
}: {
  /** Settle the question for this session. Called after every outcome, restart included. */
  onClose: () => void;
  onStatus: (s: Status) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [dontAsk, setDontAsk] = useState(false);
  const [error, setError] = useState<UiError | null>(null);
  const toast = useToast();

  /**
   * Persist the preference before acting on the button.
   *
   * Both paths run it, so ticking the box and pressing **Restart Steam** honours the tick — doing
   * it only on the dismiss path would silently drop it, which nobody notices until the prompt
   * returns next launch.
   */
  async function remember() {
    if (!dontAsk) return;
    try {
      await api.setOfferToRestartSteam(false);
    } catch {
      // A settings write failing must not stop the user restarting Steam. The worst case is being
      // asked again next time, which is the state they were already in.
    }
  }

  async function restart() {
    setBusy(true);
    setError(null);
    try {
      await remember();
      await api.restartSteam();
      toast({
        kind: 'info',
        message:
          'Restarting Steam. Your full library and instant artwork appear once it has finished ' +
          'loading.',
      });
      // Re-read, so the rest of the app is looking at the Steam that is coming up rather than the
      // one that just went down.
      onStatus(await api.status());
      onClose();
    } catch (e: unknown) {
      setError(asUiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function dismiss() {
    await remember();
    onClose();
  }

  return (
    <FocusScope name="restart-steam" onBack={() => !busy && void dismiss()}>
      <div className="modal-backdrop" role="presentation">
        <div className="modal" role="dialog" aria-modal="true" aria-label="Steam needs restarting">
          <div className="modal-head">
            <h2>Steam needs restarting</h2>
          </div>

          <p>
            Steam must be restarted in order to properly talk to its API and pull your full game
            list. Do you wish to do so now?
          </p>

          {/* An instruction rather than a claim about what Steam does. Whether Steam prompts or
              simply ends a running game on `-shutdown` has never been measured here, and the
              advice is the same either way. */}
          <p className="hint">
            Close any game you have running first &mdash; restarting Steam will end it.
          </p>

          <DontAskAgain checked={dontAsk} onChange={() => setDontAsk((v) => !v)} />

          {error && <ErrorNote error={error} />}

          <div className="row modal-actions">
            <FocusButton
              section="actions"
              row={0}
              col={0}
              className="ghost"
              autoFocus
              disabled={busy}
              onClick={() => void dismiss()}
            >
              Not now
            </FocusButton>
            <FocusButton
              section="actions"
              row={0}
              col={1}
              disabled={busy}
              onClick={() => void restart()}
            >
              {/* The busy label earns its place here more than it does on the start prompt: a
                  shutdown polls for up to 45 seconds, so without it the dialog looks frozen. */}
              {busy ? 'Restarting…' : 'Restart Steam'}
            </FocusButton>
          </div>
        </div>
      </div>
    </FocusScope>
  );
}

/**
 * Its own section, so the pad reaches it above the buttons rather than beside them.
 *
 * A checkbox rather than the `Switch` the settings screen uses, and the distinction is the point:
 * nothing is written until a button is pressed, so this is a choice something else acts on.
 */
function DontAskAgain({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('dont-ask', 0, 0);
  return (
    <label className={`toggle${focused ? ' focused' : ''}`}>
      <input ref={ref} type="checkbox" checked={checked} onChange={onChange} />
      Don&rsquo;t ask again
    </label>
  );
}
