import { invoke } from '@tauri-apps/api/core';
import { ASSET_LABEL, ASSET_TYPES, DEFAULT_LOGO_POSITION, defaultFilters, filtersToQuery } from '@sgdb/shared';
import { useEffect, useState } from 'react';

/**
 * M0 shell.
 *
 * Deliberately not a mock of the real UI — it exists to prove three wiring facts and then get
 * replaced in M3: the window opens, `@sgdb/shared` resolves across the workspace, and the
 * Rust `invoke` bridge answers.
 */
export function App() {
  const [bundleLen, setBundleLen] = useState<number | null>(null);
  const [bridgeError, setBridgeError] = useState<string | null>(null);

  useEffect(() => {
    invoke<number>('bpm_bundle_len')
      .then(setBundleLen)
      .catch((e: unknown) => setBridgeError(String(e)));
  }, []);

  const sampleQuery = filtersToQuery(defaultFilters('grid_p'));

  return (
    <main>
      <header>
        <h1>SteamGridDB Artwork Manager</h1>
        <p className="sub">M0 shell — the real library view lands in M3.</p>
      </header>

      <section>
        <h2>Wiring checks</h2>
        <dl>
          <dt>Rust bridge</dt>
          <dd>
            {bridgeError ? (
              <span className="bad">failed: {bridgeError}</span>
            ) : bundleLen === null ? (
              'checking…'
            ) : (
              <span className="ok">
                ok — BPM bundle embedded, {bundleLen} bytes
                {bundleLen < 200 ? ' (stub — run `bun run build:bpm`)' : ''}
              </span>
            )}
          </dd>

          <dt>@sgdb/shared</dt>
          <dd className="ok">
            ok — {ASSET_TYPES.length} asset types: {ASSET_TYPES.map((t) => ASSET_LABEL[t]).join(', ')}
          </dd>

          <dt>Default grid_p query</dt>
          <dd>
            <code>
              {Object.entries(sampleQuery)
                .map(([k, v]) => `${k}=${v}`)
                .join('&')}
            </code>
          </dd>

          <dt>Default logo position</dt>
          <dd>
            <code>{JSON.stringify(DEFAULT_LOGO_POSITION)}</code>
          </dd>
        </dl>
      </section>

      <section>
        <h2>Next</h2>
        <p>
          The M1 spike. <strong>S2</strong> first — capture <code>__webpack_require__</code> from
          Steam's <code>SharedJSContext</code>, find a gamepad-focusable component, and move
          controller focus onto it. Everything in the Big Picture deliverable is downstream of
          that one answer.
        </p>
      </section>
    </main>
  );
}
