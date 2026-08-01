/**
 * What Griddle found on this machine, and whether live apply is available.
 */
import { useState } from 'react';
import { api, asUiError, type LiveApplyCheck, type Status, type UiError } from '../../api';
import { ErrorNote, FocusButton, Spinner } from '../../components';

/**
 * Diagnostics is a shipped feature, not a developer tool.
 *
 * Almost every failure in this product is environmental — Steam not running, a port taken, the
 * sentinel removed. This screen is the difference between an actionable bug report and "it
 * stopped working".
 *
 * The one check here is the one thing a user can actually feel: whether artwork applies without
 * restarting Steam, or whether it gets written to disk and needs one. Resist adding checks for
 * capabilities the app does not have — a green tick against a feature that does not exist is
 * worse than no panel at all.
 */
export function DiagnosticsPanel({ status }: { status: Status }) {
  const [check, setCheck] = useState<LiveApplyCheck | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UiError | null>(null);

  async function scan() {
    setBusy(true);
    setError(null);
    setCheck(null);
    try {
      setCheck(await api.liveApplyCheck());
    } catch (e: unknown) {
      setError(asUiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h2>Diagnostics</h2>
      <dl>
        <dt>Steam</dt>
        <dd>{status.steam_root ?? <span className="bad">{status.steam_error}</span>}</dd>
        <dt>Found via</dt>
        <dd>{status.steam_source ?? '—'}</dd>
        <dt>Account</dt>
        <dd>{status.account_id ?? '—'}</dd>
        <dt>Steam running</dt>
        <dd>{status.steam_running ? 'yes' : 'no'}</dd>
        {/* Reported, not offered. The panel that used to let you toggle this is gone — live
            apply is set up at startup because it is the point of the app. */}
        <dt>Live apply</dt>
        <dd>{status.sentinel_explanation}</dd>
        <dt>Known apps</dt>
        <dd>
          {status.app_types_loaded === null
            ? "Steam's app cache is unreadable — falling back to the built-in list"
            : `${status.app_types_loaded}`}
        </dd>
        <dt>Cache</dt>
        <dd>{(status.cache_bytes / 1024 / 1024).toFixed(1)} MB</dd>
      </dl>

      <div className="row" style={{ marginTop: '1rem' }}>
        <FocusButton
          section="diagnostics"
          row={0}
          col={0}
          className="ghost"
          disabled={busy}
          onClick={() => void scan()}
        >
          {busy ? 'Testing…' : 'Test live apply'}
        </FocusButton>
      </div>

      {busy && <Spinner label="Connecting to Steam…" />}
      {/* A failed connection is the *expected* outcome when Steam is closed or has not restarted
          since the sentinel appeared, so it is reported here as information rather than as a
          fault. The apply path still works either way — it writes files instead. */}
      {error && <ErrorNote error={error} />}

      {check && (
        <ul className="features">
          <li>
            <span className={check.can_apply ? 'ok' : 'bad'}>{check.can_apply ? '✓' : '✕'}</span>{' '}
            Live apply
            <span className="hint">
              {check.can_apply
                ? ' — artwork changes immediately, with no Steam restart'
                : " — Steam's artwork API isn't available; artwork will be written to disk and needs a Steam restart to show"}
            </span>
          </li>
          {/* Not a capability, and deliberately not a ✓/✕ row: it is the build number to quote in
              a bug report. Nothing in this app varies by it. */}
          {check.clstamp && <li className="hint">Steam build {check.clstamp}</li>}
        </ul>
      )}
    </section>
  );
}
