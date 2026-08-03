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
 *
 * # Every row earns its place, and four did not
 *
 * The test is narrow: **does this help a bug report, or help the user act?** Applied before the
 * first release, it removed four rows and added the one the docs had been promising:
 *
 * | Was | Why it went |
 * |---|---|
 * | `Steam running: yes/no` | A snapshot taken at startup, rendered as though it were current — and **Live apply already says it**, as "Live apply is on, but Steam isn't running". A duplicate in the worse form. |
 * | `Known apps: 2930` | A parser statistic with nothing to compare it against. Only the *unreadable* case explains anything, so only that case is left. |
 * | `Cache: 4.2 MB` | Nothing clears it and nothing needs to: the cache is LRU-capped at 512 MB and manages itself. A number the user could neither act on nor worry about. |
 * | `Found via` as its own row | Kept, but folded in beside the path — a bare registry key path reads as internals when it stands alone. |
 *
 * **Version was missing, and `notes/troubleshooting.md` told people to include it.** That is the
 * drift worth naming: the docs described a panel nobody had checked against the panel. It reads
 * `0.0.0` on a development build, which is not a placeholder — the git tag is the source of
 * truth and the release job stamps it in, so `0.0.0` is the true statement "not built from a
 * tag".
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
        {/* First, because it is the first thing a bug report needs. */}
        <dt>Version</dt>
        <dd>{status.app_version}</dd>
        <dt>Steam</dt>
        <dd>
          {status.steam_root ?? <span className="bad">{status.steam_error}</span>}
          {/* Which registry key produced that path. It matters for exactly one failure — the
              wrong Steam of two installs — so it rides along with the path rather than taking a
              row and reading as internals. */}
          {status.steam_root && status.steam_source && (
            <span className="hint"> (found via {status.steam_source})</span>
          )}
        </dd>
        <dt>Account</dt>
        <dd>{status.account_id ?? '—'}</dd>
        {/* Reported, not offered. The panel that used to let you toggle this is gone — live
            apply is set up at startup because it is the point of the app.

            This row also carries whether Steam is running: `explain()` says "Live apply is on,
            but Steam isn't running", which is what the separate yes/no row was for. */}
        <dt>Live apply</dt>
        <dd>{status.sentinel_explanation}</dd>
        {/* Only the failure. A count of apps read is a parser statistic; a library that is
            missing games because the cache would not parse is something to report. */}
        {status.app_types_loaded === null && (
          <>
            <dt>Steam&rsquo;s app list</dt>
            <dd className="bad">Unreadable. Falling back to the built-in list.</dd>
          </>
        )}
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
                ? '. Artwork changes immediately.'
                : '. Artwork will be written to disk and needs a Steam restart to show.'}
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
