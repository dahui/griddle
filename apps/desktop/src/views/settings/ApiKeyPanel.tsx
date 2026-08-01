/**
 * The SteamGridDB API key: entering it, replacing it, and clearing it.
 */
import { useState } from 'react';
import { api, asUiError, type Status, type UiError } from '../../api';
import { ErrorNote, ExternalLink, FocusButton, useToast } from '../../components';
import { useFocusItem } from '../../focus';

/** Where the user generates their own key. Allowlisted in `griddle_core::browser`. */
const KEY_PAGE = 'https://www.steamgriddb.com/profile/preferences/api';

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
          <FocusButton section="key" row={1} col={0} className="ghost" onClick={() => void clear()}>
            Remove it
          </FocusButton>
        </div>
      ) : (
        <div className="row">
          <KeyInput
            value={key}
            onChange={setKey}
            onSubmit={() => {
              if (key.trim() && !busy) void save();
            }}
          />
          <FocusButton
            section="key"
            row={1}
            col={1}
            disabled={busy || !key.trim()}
            onClick={() => void save()}
          >
            {busy ? 'Checking…' : 'Save'}
          </FocusButton>
        </div>
      )}
      {error && <ErrorNote error={error} />}
    </section>
  );
}

/**
 * The API-key field.
 *
 * Enter submits, and that handler survives the focus model untouched: arrow keys navigate, but
 * left/right are surrendered whenever a text field holds focus, so the caret still moves through
 * a pasted key normally.
 */
function KeyInput({
  value,
  onChange,
  onSubmit,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
}) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('key', 1, 0);
  return (
    <input
      ref={ref}
      type="password"
      className={`search${focused ? ' focused' : ''}`}
      placeholder="Paste your API key"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') onSubmit();
      }}
    />
  );
}
