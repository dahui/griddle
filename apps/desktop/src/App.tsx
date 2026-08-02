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
import { SCREEN_DEPTH, useFocusItem, useScreenActions } from './focus';
import { NavSlotCtx } from './navSlot';

const TABS: Tab[] = ['library', 'settings'];
import { Library } from './views/Library';
import { AssetBrowser } from './views/AssetBrowser';
import { Settings } from './views/Settings';
import { Welcome } from './views/Welcome';

type Tab = 'library' | 'settings';

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<UiError | null>(null);
  const [tab, setTab] = useState<Tab>('library');
  const [selected, setSelected] = useState<LibraryEntry | null>(null);
  // Held in state rather than a ref: the views portal into this node, and a ref would not
  // re-render them once it was populated, so the first paint would have an empty nav row.
  const [navSlot, setNavSlot] = useState<HTMLDivElement | null>(null);

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

  // The outermost screen, so it answers the bumpers only when nothing more specific does — and B
  // only once every dialog and every inner screen has had its turn.
  const cycle = useCallback(
    (step: 1 | -1) =>
      setTab((t) => TABS[(TABS.indexOf(t) + step + TABS.length) % TABS.length] ?? 'library'),
    [],
  );
  useScreenActions(SCREEN_DEPTH.app, {
    // Settings is a detour from the library, so B leaves it the way the eye expects. On the
    // library itself there is nowhere further back, and B deliberately does nothing.
    onBack: tab === 'settings' ? () => setTab('library') : undefined,
    onTabPrev: () => cycle(-1),
    onTabNext: () => cycle(1),
  });

  if (error) return <Shell><ErrorNote error={error} onRetry={refresh} /></Shell>;
  if (!status) return <Shell><Spinner label="Starting up…" /></Shell>;

  // First run: nothing else is useful without a key, so ask for it rather than showing an
  // empty library the user cannot explain.
  //
  // `key_unreadable` is the second half of that condition and not an edge case: a settings file
  // copied from another Windows account has a key stored that DPAPI will not unseal here, so
  // `has_api_key` alone sends that user straight into a library where every request fails with
  // nothing on screen to explain why.
  if (!status.has_api_key || status.key_unreadable) {
    return (
      <Shell>
        <Welcome status={status} onStatus={setStatus} />
      </Shell>
    );
  }

  return (
    <Shell>
      <NavSlotCtx.Provider value={navSlot}>
        <nav className="tabs">
          <div className="tab-group">
            <NavTab col={0} active={tab === 'library'} onClick={() => setTab('library')}>
              Library
            </NavTab>
            <NavTab col={1} active={tab === 'settings'} onClick={() => setTab('settings')}>
              Settings
            </NavTab>
          </div>
          {/* The other half of this row's `space-between`. Filled by whichever view is showing —
              see `navSlot`. Empty on Settings, which has nothing to size. */}
          <div className="nav-actions" ref={setNavSlot} />
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
      </NavSlotCtx.Provider>
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
    <ToastProvider>
      <main>
        <header>
          <h1>Griddle</h1>
          <p className="sub">Artwork for your Steam library.</p>
        </header>
        {children}
      </main>
    </ToastProvider>
  );
}
