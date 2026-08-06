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
import { entryKey, Library } from './views/Library';
import { AssetBrowser } from './views/AssetBrowser';
import { Settings } from './views/Settings';
import { RestartSteamPrompt } from './views/RestartSteamPrompt';
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
  // The startup question about Steam's *debugging port*, settled for this session. Separate from
  // the one above because they are different questions with different answers: Steam can perfectly
  // well be running and unreachable, which is the state creating the sentinel leaves behind.
  const [steamRestartDone, setSteamRestartDone] = useState(false);
  const [offerRestart, setOfferRestart] = useState(false);
  // Bumped once, when Steam's library list turns up after the app had already loaded without it.
  // The library reloads on a change; see `SteamListWatcher` for why this is not just a status
  // refresh.
  const [steamListToken, setSteamListToken] = useState(0);
  // The game last opened, so backing out of it returns to that tile instead of the top of the
  // list. It lives here rather than in `Library` for the reason the position is lost at all:
  // `Library` is unmounted for the whole time a game is open, so it cannot remember anything.
  const [restoreKey, setRestoreKey] = useState<string | null>(null);

  // Whether Steam was up at the moment Griddle read its *first* status.
  //
  // The restart offer keys on this rather than on the live value, and the difference is the whole
  // safety of the feature: `AutoStartSteam` and `StartSteamPrompt` both re-read the status seconds
  // after launching Steam, so `steam_running` goes true while Steam is still booting and its port
  // is legitimately closed. Offering to restart a Steam that was about to work by itself is the
  // worst outcome this feature has, and no grace period is a reliable defence against it.
  const steamWasUpAtLaunch = useRef<boolean | null>(null);

  const refresh = useCallback(() => {
    api
      .status()
      .then((s) => {
        steamWasUpAtLaunch.current ??= s.steam_running;
        setStatus(s);
        setError(null);
      })
      .catch((e: unknown) => setError(asUiError(e)));
  }, []);

  useEffect(refresh, [refresh]);

  const steamListArrived = useCallback(() => {
    setSteamListToken((t) => t + 1);
    // So the rest of the app stops describing Steam as closed.
    refresh();
  }, [refresh]);

  // Stable, or `SteamDebugWatcher`'s effect restarts its poll on every render of this component
  // and the count never reaches the threshold.
  const offerToRestart = useCallback(() => setOfferRestart(true), []);

  // Stable for the same class of reason: it is in the restoring tile's effect dependencies, and
  // an inline arrow would re-run that effect on every render of this component.
  const clearRestoreKey = useCallback(() => setRestoreKey(null), []);

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

  // Steam is up but Griddle may not be able to reach it — the state creating the debugging
  // sentinel leaves behind, because Steam reads that file only when it starts. Every condition
  // earns its place:
  //
  // - `steamWasUpAtLaunch.current === true` — never ask about a Steam that Griddle or the user
  //   started this session. That one opens its port on its own, given a moment.
  // - `!steam_error` — with Steam not *found*, there is nothing to restart.
  // - `offer_to_restart_steam` — the user's own off switch, so this cannot become a nag.
  //
  // Mutually exclusive with the two above by construction, not by luck: they all require
  // `!steam_running` from the same status object this requires `steam_running` from.
  const maybeRestartSteam =
    steamWasUpAtLaunch.current === true &&
    status.steam_running &&
    !status.steam_error &&
    !steamRestartDone &&
    status.offer_to_restart_steam;

  return (
    <Shell>
      <SteamListWatcher onArrived={steamListArrived} />
      {autoStartSteam && (
        <AutoStartSteam onDone={() => setSteamStartupDone(true)} onStatus={setStatus} />
      )}
      {offerSteam && (
        <StartSteamPrompt onClose={() => setSteamStartupDone(true)} onStatus={setStatus} />
      )}
      {maybeRestartSteam && !offerRestart && <SteamDebugWatcher onUnreachable={offerToRestart} />}
      {maybeRestartSteam && offerRestart && (
        <RestartSteamPrompt onClose={() => setSteamRestartDone(true)} onStatus={setStatus} />
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
              // Remembered before the list is asked to rebuild, so it lands on the game just
              // left rather than at the top of several hundred.
              setRestoreKey(entryKey(selected));
              setSelected(null);
              // Re-read on the way back so newly applied art shows in the list.
              refresh();
            }}
          />
        ) : (
          <Library
            onPick={setSelected}
            reloadToken={steamListToken}
            restoreKey={restoreKey}
            onRestored={clearRestoreKey}
          />
        )}
      </NavSlotCtx.Provider>
    </Shell>
  );
}

/**
 * Notices when Steam's library list turns up after Griddle has already loaded without it.
 *
 * Without this the count never changes: the library loads once, and starting Steam — whether
 * Griddle did it or the user did — leaves a list that is a few hundred games short with nothing
 * on screen to say so. The offer to start Steam made that worse rather than better, because it
 * promises a fuller library and then does not deliver one until the next launch.
 *
 * Three things about it are load-bearing:
 *
 * - **It waits for the app list, not for Steam.** The realm answers at about 3 s on a cold start
 *   and `collectionStore` is not populated until about 7 s, measured. Reloading on the earlier
 *   signal would quietly produce the offline list again.
 * - **Only a transition reloads.** If the very first poll succeeds, Steam was already up when the
 *   library loaded, the list is already the full one, and reloading would be a spinner for
 *   nothing. `sawAbsent` is what makes this a transition rather than a state.
 * - **It gives up on nothing and slows down instead.** Somebody may start Steam long after
 *   opening Griddle, so there is no deadline; the interval just stretches once the interesting
 *   window has passed.
 */
function SteamListWatcher({ onArrived }: { onArrived: () => void }) {
  const toast = useToast();

  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let sawAbsent = false;
    let polls = 0;

    async function tick() {
      if (stopped) return;
      polls += 1;
      // `steam_library_ready` never rejects, but a Tauri call can still fail on its own terms
      // and "not ready" is the right reading of that.
      const ready = await api.steamLibraryReady().catch(() => false);
      if (stopped) return;
      if (!ready) {
        sawAbsent = true;
        // Tight while the list is plausibly on its way, then lazy: the first two minutes cover
        // "Griddle just launched Steam", and after that this is only waiting on a person.
        timer = setTimeout(() => void tick(), polls < 40 ? 3000 : 15000);
        return;
      }
      // Ready. Nothing left to watch either way — the list does not need re-checking once it is
      // the full one, and a later Steam shutdown must not shrink what is on screen.
      if (sawAbsent) {
        onArrived();
        toast({ kind: 'info', message: 'Steam is ready — showing your full library.' });
      }
    }

    void tick();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  }, [onArrived, toast]);

  return null;
}

/**
 * Decides whether Steam is running without its debugging port, before anything is said about it.
 *
 * That state is entirely invisible and Griddle creates it: `.cef-enable-remote-debugging` is
 * written at every launch and Steam reads it only when it *starts*, so until the next restart
 * artwork can only be written to disk and **All games** is the offline list. Neither symptom
 * mentions the flag, and the flag was never mentioned to the user.
 *
 * Two things are load-bearing:
 *
 * - **Reachable at any point ends it for good.** The port is open, a restart would achieve
 *   nothing, and there is no question left to ask.
 * - **It takes ten polls to conclude the opposite.** The port is not open for the first few
 *   seconds of a Steam start (the realm answers at about 3 s, measured), so a single failed probe
 *   proves nothing.
 *
 * Ten polls is **not** thirty seconds, which is what this said until it was timed. A probe against
 * a port with nothing on it does not fail instantly, so the real figure measured end to end is
 * 45-51 s from launch — about a minute. That is slower than intended and deliberately not tuned
 * down: erring late costs a user half a minute of a list they were not looking at yet, and erring
 * early asks them to restart a Steam that was going to work.
 *
 * The caller additionally refuses to mount this for a Steam that came up during this session —
 * see `steamWasUpAtLaunch`. The grace period covers a Steam mid-boot; that check covers a Steam
 * Griddle started itself, which no grace period could reliably cover.
 */
function SteamDebugWatcher({ onUnreachable }: { onUnreachable: () => void }) {
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let polls = 0;

    async function tick() {
      if (stopped) return;
      polls += 1;
      // `steam_debug_ready` never rejects, but a Tauri call can still fail on its own terms and
      // "not reachable" is the right reading of that.
      const ready = await api.steamDebugReady().catch(() => false);
      if (stopped || ready) return;
      if (polls >= 10) {
        onUnreachable();
        return;
      }
      timer = setTimeout(() => void tick(), 3000);
    }

    void tick();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  }, [onUnreachable]);

  return null;
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
