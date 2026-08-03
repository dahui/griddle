/**
 * What Griddle does about Steam when it opens.
 *
 * This panel exists because both settings here can be switched off from somewhere else — the
 * offer carries a "Don't ask again" — and a preference a user can turn off with no way to turn
 * back on is a trap rather than a preference.
 *
 * The two are ordered by how much they do: starting Steam outright supersedes offering to, so
 * the offer row is hidden while automatic start is on. It is hidden rather than disabled because
 * the focus model has no notion of a skipped item, and a control the cursor can land on but not
 * focus is the swallowed-press bug this app has already fixed once. The stored value is left
 * alone either way, so switching automatic start back off restores whatever the user had.
 *
 * It is deliberately not in Diagnostics, which reports the environment and sets nothing.
 */
import { useState } from 'react';
import { api, asUiError, type Status, type UiError } from '../../api';
import { ErrorNote, Switch } from '../../components';

export function StartupPanel({
  status,
  onStatus,
}: {
  status: Status;
  onStatus: (s: Status) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UiError | null>(null);

  async function write(change: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await change();
      // Re-read rather than assume: these values live on `Status`, which the startup path
      // branches on, so the store has to be the thing that changed.
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

      <Switch
        section="startup"
        row={0}
        checked={status.auto_start_steam}
        disabled={busy}
        onChange={() => void write(() => api.setAutoStartSteam(!status.auto_start_steam))}
      >
        Start Steam automatically if it isn&rsquo;t running when Griddle launches
      </Switch>

      {!status.auto_start_steam && (
        <Switch
          section="startup"
          row={1}
          checked={status.offer_to_start_steam}
          disabled={busy}
          onChange={() => void write(() => api.setOfferToStartSteam(!status.offer_to_start_steam))}
        >
          Ask to start Steam if it is not running when Griddle launches
        </Switch>
      )}

      {error && <ErrorNote error={error} />}
    </section>
  );
}
