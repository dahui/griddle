/** API key, live apply, and diagnostics. */
import { useState } from 'react';
import { api, asUiError, type ModuleReport, type Status, type UiError } from '../api';
import { ErrorNote, ExternalLink, Spinner } from '../components';

const KEY_PAGE = 'https://www.steamgriddb.com/profile/preferences/api';

export function Settings({ status, onStatus }: { status: Status; onStatus: (s: Status) => void }) {
  return (
    <>
      <ApiKeyPanel status={status} onStatus={onStatus} />
      <DiagnosticsPanel status={status} />
    </>
  );
}

/**
 * What the app set up on its own, shown once on first run.
 *
 * 🔑 Live apply is not a setting — applying artwork without restarting Steam is the whole
 * reason this app exists, so its prerequisite is arranged rather than offered. CSS Loader and
 * Decky set the same flag and tell nobody; this at least says what it did and how to undo it.
 */
export function SetupNote() {
  return (
    <section>
      <h2>What this set up</h2>
      <p>
        So artwork can apply without restarting Steam, this app creates an empty{' '}
        <code>.cef-enable-remote-debugging</code> file in Steam&rsquo;s folder. It&rsquo;s
        Valve&rsquo;s own setting — CSS Loader and Decky use the same one — and it takes effect
        the next time Steam starts. Deleting the file undoes it.
      </p>
      <p className="hint">
        Until Steam restarts, artwork still applies; it just needs a restart to show up.
      </p>
    </section>
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
        This app uses <strong>your own</strong> API key rather than shipping a shared one. Yours
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
 * Almost every failure in this product is environmental — Steam not running, a port taken, a
 * client update moving things. This screen is the difference between an actionable bug report
 * and "it stopped working".
 */
function DiagnosticsPanel({ status }: { status: Status }) {
  const [report, setReport] = useState<ModuleReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UiError | null>(null);

  async function scan() {
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      setReport(await api.resolveModules());
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
          {busy ? 'Scanning…' : 'Check Steam compatibility'}
        </button>
      </div>

      {busy && <Spinner label="Checking Steam…" />}
      {error && <ErrorNote error={error} />}

      {report && (
        <>
          <p className="hint">
            Steam build {report.clstamp} — {report.resolved} of {report.outcomes.length}{' '}
            components found in {report.total_modules} modules.
          </p>
          <ul className="features">
            {report.features.map(([name, ok, fallback]) => (
              <li key={name}>
                <span className={ok ? 'ok' : 'bad'}>{ok ? '✓' : '✕'}</span> {name}
                {!ok && <span className="hint"> — {fallback}</span>}
              </li>
            ))}
            <li>
              <span className="ok">✓</span> Live apply
              <span className="hint"> — built into Steam, so updates don&rsquo;t break it</span>
            </li>
          </ul>
          <details>
            <summary>Component detail</summary>
            <dl className="modules">
              {report.outcomes.map(([name, detail]) => (
                <div key={name}>
                  <dt>{name}</dt>
                  <dd>
                    <code>{detail}</code>
                  </dd>
                </div>
              ))}
            </dl>
          </details>
        </>
      )}
    </section>
  );
}
