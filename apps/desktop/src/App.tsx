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
import { useCallback, useEffect, useRef, useState } from 'react';
import logo from './assets/logo.png';
import { api, asUiError, type LibraryEntry, type Status, type UiError } from './api';
import { ErrorNote, Spinner, ToastProvider, useErrorToast, useToast } from './components';
import { SCREEN_DEPTH, useFocusItem, useScreenActions } from './focus';
import { NavSlotCtx } from './navSlot';

const TABS: Tab[] = ['library', 'settings'];
import { Library } from './views/Library';
import { AssetBrowser } from './views/AssetBrowser';
import { Settings } from './views/Settings';
import { StartSteamPrompt } from './views/StartSteamPrompt';
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
  // The startup question about Steam, settled for *this session* -- by dismissing the offer, or by
  // starting Steam automatically. Separate from the stored preferences so "Not now" silences the
  // prompt until the next launch without writing anything, and a later `refresh()` -- which
  // re-reads a status that still says Steam is closed, because starting it takes tens of seconds
  // -- cannot bring the question back mid-session.
  const [steamStartupDone, setSteamStartupDone] = useState(false);

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
    // `bare`: the welcome screen shows the wordmark itself, so the app header would be the same
    // logo twice on the one screen where there is nothing else to look at.
    return (
      <Shell bare>
        <Welcome status={status} onStatus={setStatus} />
      </Shell>
    );
  }

  // Steam is not running and something should happen about it. Every condition earns its place:
  //
  // - `!steam_running` — the only reason to do anything at all.
  // - `!steamStartupDone` — already settled this session.
  // - `!steam_error` — with Steam not *found*, the welcome screen and the empty library already
  //   explain the situation, and launching something we cannot locate would be absurd.
  //
  // Placed after the first-run gate on purpose: a new user meets one screen, not a dialog stacked
  // on top of the screen asking for their API key.
  const steamAbsent = !status.steam_running && !steamStartupDone && !status.steam_error;
  // Starting it outright supersedes offering to, so the two are exclusive here rather than in the
  // settings that feed them. `offer_to_start_steam` keeps whatever the user chose, and turning
  // automatic start off restores it.
  const autoStartSteam = steamAbsent && status.auto_start_steam;
  const offerSteam = steamAbsent && !status.auto_start_steam && status.offer_to_start_steam;

  return (
    <Shell>
      {autoStartSteam && (
        <AutoStartSteam onDone={() => setSteamStartupDone(true)} onStatus={setStatus} />
      )}
      {offerSteam && (
        <StartSteamPrompt onClose={() => setSteamStartupDone(true)} onStatus={setStatus} />
      )}
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

/**
 * Starts Steam without asking, when the user has said to.
 *
 * Renders nothing. It is a component rather than an effect in `App` for one reason: the toast
 * host lives inside `Shell`, so only something rendered under it can raise a message — and this
 * must raise one. A program launching another program silently is the sort of thing that reads
 * as a bug when you notice it in Task Manager.
 *
 * The ref guard, not the mount, is what makes it fire once. A status refresh while Steam is still
 * coming up leaves this mounted, and React's development double-effect would otherwise launch
 * Steam twice.
 */
function AutoStartSteam({
  onDone,
  onStatus,
}: {
  /** Settle the question for this session, whatever the outcome. */
  onDone: () => void;
  onStatus: (s: Status) => void;
}) {
  const toast = useToast();
  const errorToast = useErrorToast();
  const fired = useRef(false);

  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    void (async () => {
      try {
        await api.startSteam();
        toast({
          kind: 'info',
          message: 'Steam wasn’t running, so Griddle started it. Your full library appears once ' +
            'it has finished loading.',
        });
        // Re-read so the rest of the app stops believing Steam is closed.
        onStatus(await api.status());
      } catch (e: unknown) {
        // A toast, not a blocking error: everything on screen still works, it just works in the
        // Steam-closed way.
        errorToast(e);
      } finally {
        onDone();
      }
    })();
  }, [toast, errorToast, onDone, onStatus]);

  return null;
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
function Shell({
  children,
  bare,
}: {
  children: React.ReactNode;
  /** Drop the header, for a screen that shows the wordmark itself. */
  bare?: boolean;
}) {
  return (
    <ToastProvider>
      <main>
        {!bare && (
          <header>
            {/* The image *is* the heading, so it keeps the `h1` and `alt` carries the name -- the
                opposite of the decorative `alt=""` on the welcome screen, where a real heading
                sits beside it. Dimensions are explicit so the header does not reflow when it
                decodes.

                The same file the welcome screen uses, deliberately. The artwork is never cropped
                or recomposed now, so a header-sized copy would be a second encoding of an image
                already in the bundle -- the browser downscales this one and it costs nothing. */}
            <h1>
              <img className="brand" src={logo} alt="Griddle" width={128} height={84} />
            </h1>
          </header>
        )}
        {children}
      </main>
    </ToastProvider>
  );
}
