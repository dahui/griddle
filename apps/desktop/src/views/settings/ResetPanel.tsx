/**
 * Removing every piece of custom artwork, and the confirmation that guards it.
 */
import { useState } from 'react';
import { api, asUiError, type ResetPlan, type UiError } from '../../api';
import { ErrorNote, FocusButton, useErrorToast, useToast } from '../../components';
import { FocusScope } from '../../focus';

/**
 * Remove every piece of custom artwork at once.
 *
 * The only bulk *destructive* action in the product, and the one place a confirmation is
 * genuinely earned. Two things make it safe rather than merely guarded:
 *
 * - **The counts are measured, not estimated.** `resetAllPlan` is a read-only command run when
 *   the button is pressed, so the dialog quotes what is actually on disk right now. Naming every
 *   file — what the per-game reset does — is useless at this scale, so counts stand in for it.
 * - **Nothing opens if nothing would happen.** With no custom artwork the button reports that and
 *   stops, rather than presenting a dialog whose confirm button would do nothing.
 */
export function ResetAllPanel() {
  const [plan, setPlan] = useState<ResetPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const toastError = useErrorToast();

  async function check() {
    setBusy(true);
    try {
      const p = await api.resetAllPlan();
      // "We looked, and there was nothing" has to be said out loud, or the button reads as
      // broken — it is the one outcome with no visible consequence.
      if (p.files === 0) {
        toast({ kind: 'info', message: 'No custom artwork to reset.' });
      } else {
        setPlan(p);
      }
    } catch (e: unknown) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  }

  async function confirm() {
    setBusy(true);
    try {
      const result = await api.resetAllArt();
      const games = `${result.games} ${result.games === 1 ? 'game' : 'games'}`;
      // Partial failure is never folded into a success message: it names the games and stays
      // the full error duration.
      toast(
        result.failed.length > 0
          ? {
              kind: 'bad',
              message: `Reset ${games}, but ${result.failed.length} could not be removed.`,
              action: result.failed.join(', '),
            }
          : {
              kind: result.method === 'live' ? 'ok' : 'info',
              message: `Reset ${games}.${result.needs_restart ? ' Restart Steam to see it.' : ''}`,
              action: result.fell_back_because,
            },
      );
    } catch (e: unknown) {
      toastError(e);
    } finally {
      setBusy(false);
      setPlan(null);
    }
  }

  return (
    <section>
      <h2>Reset all artwork</h2>
      <p>Removes all custom artwork, reverting all games back to Steam&rsquo;s default.</p>

      <div className="row">
        <FocusButton section="reset" row={0} col={0} className="danger" disabled={busy} onClick={() => void check()}>
          {busy ? 'Working…' : 'Reset all artwork…'}
        </FocusButton>
      </div>

      {plan && (
        <ConfirmReset
          plan={plan}
          busy={busy}
          onCancel={() => setPlan(null)}
          onConfirm={() => void confirm()}
        />
      )}
    </section>
  );
}

/**
 * The confirmation.
 *
 * The confirm button restates the consequence *at the moment of committing* rather than saying
 * "OK" — a count on the button is the last thing read before the click. Cancel is the plain
 * action and is focused first, so the dangerous one is never the default.
 */
function ConfirmReset({
  plan,
  busy,
  onCancel,
  onConfirm,
}: {
  plan: ResetPlan;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const games = `${plan.games} ${plan.games === 1 ? 'game' : 'games'}`;
  const files = `${plan.files} ${plan.files === 1 ? 'file' : 'files'}`;

  // Escape cancels. It did not before — this dialog had no keyboard dismissal at all, which for
  // the app's one destructive action is the worst place to leave that gap.
  return (
    <FocusScope name="confirm-reset" onBack={() => !busy && onCancel()}>
    <div
      className="modal-backdrop"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onCancel();
      }}
    >
      <div className="modal" role="dialog" aria-modal="true" aria-label="Reset all artwork">
        <div className="modal-head">
          <h2>Reset all artwork?</h2>
        </div>

        <p>
          This deletes your custom artwork for <strong>{games}</strong> — {files} in
          Steam&rsquo;s <code>grid</code> folder — and puts Steam&rsquo;s own artwork back.
        </p>
        <p className="note note-bad">
          <strong>This can&rsquo;t be undone.</strong> Artwork applied by other tools lives in the
          same folder, so it goes too.
        </p>
        <p className="hint">
          Steam&rsquo;s own artwork is stored separately and isn&rsquo;t touched, so every game
          keeps a picture.
        </p>

        <div className="row modal-actions">
          {/* Cancel is first *and* focused first, so the dangerous button is never what a
              reflexive Enter or A-button press lands on. */}
          <FocusButton
            section="actions"
            row={0}
            col={0}
            className="ghost"
            autoFocus
            disabled={busy}
            onClick={onCancel}
          >
            Cancel
          </FocusButton>
          <FocusButton
            section="actions"
            row={0}
            col={1}
            className="danger"
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? 'Removing…' : `Remove ${files}`}
          </FocusButton>
        </div>
      </div>
    </div>
    </FocusScope>
  );
}
