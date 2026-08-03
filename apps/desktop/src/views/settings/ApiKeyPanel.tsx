/**
 * The SteamGridDB API key in Settings: replacing it, and clearing it.
 *
 * Entering one is [`KeyEntry`]'s job, shared with the first-run screen so the two cannot drift
 * apart. What differs here is the framing: this reader already has the app, so the panel explains
 * the policy and offers the key as something to manage, rather than walking anybody through
 * obtaining one.
 */
import { useState } from 'react';
import { api, type Status, type UiError } from '../../api';
import { ErrorNote, ExternalLink, FocusButton, KeyEntry } from '../../components';

/** Where the user generates their own key. Allowlisted in `griddle_core::browser`. */
const KEY_PAGE = 'https://www.steamgriddb.com/profile/preferences/api';

export function ApiKeyPanel({
  status,
  onStatus,
}: {
  status: Status;
  onStatus: (s: Status) => void;
}) {
  const [error, setError] = useState<UiError | null>(null);

  async function clear() {
    await api.clearApiKey();
    onStatus(await api.status());
  }

  // Stored is not the same as usable: a settings file from another Windows account decrypts
  // nowhere else. Saying "Key saved." over one of those would describe a key that cannot make a
  // single request.
  const usable = status.has_api_key && !status.key_unreadable;

  return (
    <section>
      <h2>SteamGridDB API key</h2>
      <p>
        Stored encrypted for your Windows account, and only ever sent to SteamGridDB. Grab one
        from{' '}
        <ExternalLink href={KEY_PAGE} onError={setError}>
          your SteamGridDB preferences
        </ExternalLink>
        .
      </p>

      {status.key_unreadable && (
        <p className="note note-bad">
          The saved key could not be decrypted on this Windows account. Paste it again.
        </p>
      )}

      {usable ? (
        <div className="row">
          <span className="ok">Key saved.</span>
          <FocusButton section="key" row={1} col={0} className="ghost" onClick={() => void clear()}>
            Remove it
          </FocusButton>
        </div>
      ) : (
        <KeyEntry onStatus={onStatus} />
      )}
      {error && <ErrorNote error={error} />}
    </section>
  );
}
