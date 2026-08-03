/**
 * Offered once at startup, when Steam is not running.
 *
 * **Griddle works without Steam**, so this is never a blocker and never gates the library. What it
 * exists for is that three things are quietly worse with Steam closed, and none of them announces
 * itself: artwork needs a restart to show, **All games** is a few hundred games short, and
 * refunded titles stay in the list. A user who never starts Steam first gets all three and no
 * reason to suspect any of them.
 *
 * **It must not become startup furniture.** This project deleted a setup dialog once already for
 * being unnecessary, and a prompt that reappears every launch for somebody who deliberately runs
 * Steam-less is the same mistake. Hence "Don't ask again", which writes
 * `offer_to_start_steam: false` and is the whole reason the setting exists. Dismissing without it
 * silences the prompt for this session only.
 *
 * Cancel is focused first and the launch button is second, matching `ConfirmReset`: the action
 * with a side effect is never what a reflexive Enter or A press lands on.
 */
import { useState } from 'react';
import { api, asUiError, type Status, type UiError } from '../api';
import { ErrorNote, FocusButton, useToast } from '../components';
import { FocusScope, useFocusItem } from '../focus';

export function StartSteamPrompt({
  onClose,
  onStatus,
}: {
  /** Dismiss for this session. Called after every outcome, including a successful launch. */
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
   * Both paths run it, so ticking the box and pressing **Start Steam** honours the tick — doing it
   * only on the dismiss path would silently drop it, which is the sort of thing nobody notices
   * until the prompt returns next launch.
   */
  async function remember() {
    if (!dontAsk) return;
    try {
      await api.setOfferToStartSteam(false);
    } catch {
      // A settings write failing must not stop the user starting Steam. The worst case is being
      // asked again next time, which is the state they were already in.
    }
  }

  async function start() {
    setBusy(true);
    setError(null);
    try {
      await remember();
      await api.startSteam();
      // Deliberately does not wait for Steam to be ready: that is tens of seconds, and every
      // live feature re-checks on use, so the app upgrades itself as Steam comes up.
      toast({
        kind: 'info',
        message: 'Starting Steam. Your full library appears once it has finished loading.',
      });
      // Re-read so the rest of the app stops believing Steam is closed.
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
    <FocusScope name="start-steam" onBack={() => !busy && void dismiss()}>
      <div className="modal-backdrop" role="presentation">
        <div className="modal" role="dialog" aria-modal="true" aria-label="Steam is not running">
          <div className="modal-head">
            <h2>Steam isn&rsquo;t running</h2>
          </div>

          <p>
            Griddle works either way, but with Steam open your artwork appears immediately instead
            of at the next restart, and <strong>All games</strong> shows your whole library.
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
              onClick={() => void start()}
            >
              {busy ? 'Starting…' : 'Start Steam'}
            </FocusButton>
          </div>
        </div>
      </div>
    </FocusScope>
  );
}

/** Its own section, so the pad reaches it above the buttons rather than beside them. */
function DontAskAgain({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('dont-ask', 0, 0);
  return (
    <label className={`toggle${focused ? ' focused' : ''}`}>
      <input ref={ref} type="checkbox" checked={checked} onChange={onChange} />
      Don&rsquo;t ask again
    </label>
  );
}
