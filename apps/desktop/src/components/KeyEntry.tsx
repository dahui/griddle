/**
 * Entering an API key: the field, the Save button, validation and the error.
 *
 * Extracted because two screens take a key — the first-run welcome and Settings — and they need
 * to say **different things** around an **identical** mechanism. Before this, first run simply
 * rendered the Settings panel, which is why it opened with "SteamGridDB API key" under a second
 * heading and explained the licensing policy to someone who had not yet been told what to do.
 *
 * So this component owns no prose at all. Both callers supply their own.
 */
import { useEffect, useRef, useState } from 'react';
import { looksLikeApiKey } from '@griddle/shared';
import { api, asUiError, type Status, type UiError } from '../api';
import { useFocusItem } from '../focus';
import { ErrorNote, FocusButton } from './primitives';

export function KeyEntry({
  onStatus,
  autoFocus,
}: {
  onStatus: (s: Status) => void;
  /** First run puts the caret here; Settings does not steal focus from the page. */
  autoFocus?: boolean;
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

  // Shown, never enforced. `ApiKey::new` accepts a key of any shape on purpose — today's 32-hex
  // form is observed, not guaranteed, and refusing a future one would brick the app. Blocking
  // Save here would impose exactly the rule Rust declined to impose, so the button stays live and
  // this is only a remark.
  const odd = key.trim().length > 0 && !looksLikeApiKey(key);

  return (
    <>
      <div className="row">
        <KeyInput
          value={key}
          onChange={setKey}
          autoFocus={autoFocus}
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
      {odd && !error && (
        <p className="hint">
          That doesn’t look like a SteamGridDB key — they are 32 letters and numbers. Save it
          anyway if you’re sure.
        </p>
      )}
      {error && <ErrorNote error={error} />}
    </>
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
  autoFocus,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
  autoFocus?: boolean;
}) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('key', 1, 0);

  // Focused from a microtask, and both halves of that are load-bearing.
  //
  // The focus model learns about mouse and Tab focus from a `focusin` listener, which looks the
  // element up in the registry and moves the cursor to it. Two orderings have to hold for that to
  // work, and the obvious ways to write this break one each:
  //
  //  - React's `autoFocus` attribute fires during commit, *before* any passive effect, so the
  //    control is not registered yet and the lookup finds nothing.
  //  - `.focus()` straight from an effect is too early too: React runs child effects before
  //    parent effects, so the provider — which is an ancestor — has not installed the `focusin`
  //    listener at that point. There is nothing listening at all.
  //
  // A microtask queued from here runs after the whole effect pass, by which time this control is
  // registered *and* the listener exists.
  //
  // Getting it wrong is close to invisible: the caret blinks in the field either way, so it looks
  // focused, and only the first D-pad press shows the cursor was never set — it jumps somewhere
  // unrelated. `Welcome.test.tsx` asserts the model's own `.focused` class rather than
  // `document.activeElement`, because only the former can tell these apart.
  const wanted = useRef(autoFocus);
  useEffect(() => {
    if (!wanted.current) return;
    queueMicrotask(() => ref.current?.focus());
  }, [ref]);

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
