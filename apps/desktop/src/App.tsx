/**
 *
 * Library list with current art, the five asset tabs, SteamGridDB browsing with infinite
 * scroll, and apply. The apply path tries live first and falls back to writing files; the UI's
 * only job there is to say clearly whether a Steam restart is needed.
 *
 * The selected asset tab lives in `AssetBrowser`, which is unmounted whenever the list is
 * showing. That is what makes every game open on the Capsule tab: the reset is structural, not
 * something this component has to remember to do when a game is picked.
 */
import { useCallback, useEffect, useState } from 'react';
import { api, asUiError, type LibraryEntry, type Status, type UiError } from './api';
import { ErrorNote, Spinner, ToastProvider } from './components';
import { FocusProvider, useFocusItem } from './focus';
import { Library } from './views/Library';
import { AssetBrowser } from './views/AssetBrowser';
import { ApiKeyPanel, Settings } from './views/Settings';

type Tab = 'library' | 'settings';

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<UiError | null>(null);
  const [tab, setTab] = useState<Tab>('library');
  const [selected, setSelected] = useState<LibraryEntry | null>(null);

  const refresh = useCallback(() => {
    api
      .status()
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e: unknown) => setError(asUiError(e)));
  }, []);

  useEffect(refresh, [refresh]);

  if (error) return <Shell><ErrorNote error={error} onRetry={refresh} /></Shell>;
  if (!status) return <Shell><Spinner label="Starting up…" /></Shell>;

  // First run: nothing else is useful without a key, so ask for it rather than showing an
  // empty library the user cannot explain.
  if (!status.has_api_key) {
    return (
      <Shell>
        <section className="welcome">
          <h2>Welcome</h2>
          <p>
            Browse SteamGridDB and apply artwork to your Steam library. You&rsquo;ll need a
            SteamGridDB API key to get started.
          </p>
        </section>
        <ApiKeyPanel status={status} onStatus={setStatus} />
      </Shell>
    );
  }

  return (
    <Shell>
      <nav className="tabs">
        <div className="tab-group">
          <NavTab col={0} active={tab === 'library'} onClick={() => setTab('library')}>
            Library
          </NavTab>
          <NavTab col={1} active={tab === 'settings'} onClick={() => setTab('settings')}>
            Settings
          </NavTab>
        </div>
      </nav>

      {tab === 'settings' ? (
        <Settings status={status} onStatus={setStatus} />
      ) : selected ? (
        <AssetBrowser
          entry={selected}
          onBack={() => {
            setSelected(null);
            // Re-read on the way back so newly applied art shows in the list.
            refresh();
          }}
        />
      ) : (
        <Library onPick={setSelected} />
      )}
    </Shell>
  );
}

/** The Library/Settings switch — the app's top navigation section, so `row 0` of `nav`. */
function NavTab({
  col,
  active,
  onClick,
  children,
}: {
  col: number;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('nav', 0, col);
  return (
    <button
      ref={ref}
      type="button"
      className={`tab${active ? ' active' : ''}${focused ? ' focused' : ''}`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/**
 * The frame every screen sits in.
 *
 * The toast host lives here rather than around one view, so a confirmation raised just before a
 * navigation is not unmounted along with the thing that raised it. `FocusProvider` wraps it in
 * turn, because the focus model has to outlive any single screen: an overlay opened on the last
 * one still needs somewhere to hand focus back to.
 */
function Shell({ children }: { children: React.ReactNode }) {
  return (
    <FocusProvider>
      <ToastProvider>
        <main>
          <header>
            <h1>Griddle</h1>
            <p className="sub">Artwork for your Steam library.</p>
          </header>
          {children}
        </main>
      </ToastProvider>
    </FocusProvider>
  );
}
