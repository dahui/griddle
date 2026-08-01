/** API key, resetting artwork, and diagnostics. */
import { useState } from 'react';
import {
  api,
  asUiError,
  type LiveApplyCheck,
  type ResetPlan,
  type Status,
  type UiError,
} from '../api';
import { ErrorNote, ExternalLink, Spinner, useErrorToast, useToast } from '../components';

const KEY_PAGE = 'https://www.steamgriddb.com/profile/preferences/api';

export function Settings({ status, onStatus }: { status: Status; onStatus: (s: Status) => void }) {
  return (
    <>
      <ApiKeyPanel status={status} onStatus={onStatus} />
      <ResetAllPanel />
      <DiagnosticsPanel status={status} />
    </>
  );
}

/**
 * Remove every piece of custom artwork at once.
 *
 * 🔴 The only bulk *destructive* action in the product, and the one place a confirmation is
 * genuinely earned. Two things make it safe rather than merely guarded:
 *
 * - **The counts are measured, not estimated.** `resetAllPlan` is a read-only command run when
 *   the button is pressed, so the dialog quotes what is actually on disk right now. Naming every
 *   file — what the per-game reset does — is useless at this scale, so counts stand in for it.
 * - **Nothing opens if nothing would happen.** With no custom artwork the button reports that and
 *   stops, rather than presenting a dialog whose confirm button would do nothing.
 */
function ResetAllPanel() {
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
        <button type="button" className="danger" disabled={busy} onClick={() => void check()}>
          {busy ? 'Working…' : 'Reset all artwork…'}
        </button>
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

  return (
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
          <button type="button" className="ghost" autoFocus disabled={busy} onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="danger" disabled={busy} onClick={onConfirm}>
            {busy ? 'Removing…' : `Remove ${files}`}
          </button>
        </div>
      </div>
    </div>
  );
}


export function ApiKeyPanel({
  status,
  onStatus,
}: {
  status: Status;
  onStatus: (s: Status) => void;
}) {
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UiError | null>(null);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      // Validated against the live API before it is stored, so a wrong key is rejected here
      // rather than turning into a 401 on every later request.
      await api.setApiKey(key);
      setKey('');
      onStatus(await api.status());
    } catch (e: unknown) {
      setError(asUiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    await api.clearApiKey();
    onStatus(await api.status());
  }

  return (
    <section>
      <h2>SteamGridDB API key</h2>
      <p>
        This app uses <strong>your own</strong> API key rather than shipping a shared one. It
        is stored encrypted for your Windows account and only ever sent to SteamGridDB.
      </p>
      <p>
        Grab one from{' '}
        <ExternalLink href={KEY_PAGE} onError={setError}>
          your SteamGridDB preferences
        </ExternalLink>
        .
      </p>

      {status.has_api_key ? (
        <div className="row">
          <span className="ok">Key saved.</span>
          <button type="button" className="ghost" onClick={() => void clear()}>
            Remove it
          </button>
        </div>
      ) : (
        <div className="row">
          <input
            type="password"
            className="search"
            placeholder="Paste your API key"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && key.trim() && !busy) void save();
            }}
          />
          <button type="button" disabled={busy || !key.trim()} onClick={() => void save()}>
            {busy ? 'Checking…' : 'Save'}
          </button>
        </div>
      )}
      {error && <ErrorNote error={error} />}
    </section>
  );
}

/**
 * Diagnostics is a shipped feature, not a developer tool.
 *
 * Almost every failure in this product is environmental — Steam not running, a port taken, the
 * sentinel removed. This screen is the difference between an actionable bug report and "it
 * stopped working".
 *
 * 🔴 The check used to be **"Check Steam compatibility"**, grading eleven structural module
 * finders and reporting ✓/✕ against three named features: *Big Picture UI*, *Context-menu entry*
 * and *Zoom slider*. All three belonged to the Big Picture deliverable, which was never built and
 * is now cut — so the panel was reporting availability for capabilities the app does not have.
 * A green tick against a feature that does not exist is worse than no panel at all.
 *
 * What replaces it is the one thing the user can actually feel: whether artwork applies without
 * restarting Steam, or whether it gets written to disk and needs one.
 */
function DiagnosticsPanel({ status }: { status: Status }) {
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
        <button type="button" className="ghost" disabled={busy} onClick={() => void scan()}>
          {busy ? 'Testing…' : 'Test live apply'}
        </button>
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
