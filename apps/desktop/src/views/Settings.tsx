/**
 * The settings screen: three independent panels, one per file.
 *
 * This file is only their order on the page. The API key comes first because on a first run it is
 * the only one that matters; diagnostics comes last because it is what a bug report needs rather
 * than something anyone sets.
 */
import type { Status } from '../api';
import { ApiKeyPanel } from './settings/ApiKeyPanel';
import { DiagnosticsPanel } from './settings/DiagnosticsPanel';
import { ResetAllPanel } from './settings/ResetPanel';
import { StartupPanel } from './settings/StartupPanel';

// Re-exported because the first-run flow in `App` shows the key panel on its own, before the
// settings screen exists as somewhere to navigate to.
export { ApiKeyPanel } from './settings/ApiKeyPanel';

export function Settings({ status, onStatus }: { status: Status; onStatus: (s: Status) => void }) {
  return (
    <>
      <ApiKeyPanel status={status} onStatus={onStatus} />
      <StartupPanel status={status} onStatus={onStatus} />
      <ResetAllPanel />
      <DiagnosticsPanel status={status} />
    </>
  );
}
