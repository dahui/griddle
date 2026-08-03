/**
 * The one startup preference: whether to offer to launch Steam.
 *
 * This panel exists because the offer carries a **"Don't ask again"**, and a preference a user can
 * switch off with no way to switch back on is a trap rather than a preference. That is the whole
 * justification for the section — not that the setting is interesting, but that turning it off
 * must be reversible somewhere the user can find.
 *
 * It is deliberately not in Diagnostics, which reports the environment and sets nothing.
 */
import { useState } from 'react';
import { api, asUiError, type Status, type UiError } from '../../api';
import { ErrorNote } from '../../components';
import { useFocusItem } from '../../focus';

export function StartupPanel({
  status,
  onStatus,
}: {
  status: Status;
  onStatus: (s: Status) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UiError | null>(null);
  const { ref, focused } = useFocusItem<HTMLInputElement>('startup', 0, 0);

  async function toggle() {
    setBusy(true);
    setError(null);
    try {
      await api.setOfferToStartSteam(!status.offer_to_start_steam);
      // Re-read rather than assume: this value lives on `Status`, which several screens branch
      // on, so the store has to be the thing that changed.
      onStatus(await api.status());
    } catch (e: unknown) {
      setError(asUiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h2>Startup</h2>
      <p>
        Griddle works with Steam closed, but artwork then needs a Steam restart to show and{' '}
        <strong>All games</strong> is missing anything you have never launched on this PC.
      </p>

      <label className={`toggle${focused ? ' focused' : ''}`}>
        <input
          ref={ref}
          type="checkbox"
          checked={status.offer_to_start_steam}
          disabled={busy}
          onChange={() => void toggle()}
        />
        Offer to start Steam if it isn&rsquo;t running
      </label>

      {error && <ErrorNote error={error} />}
    </section>
  );
}
