/**
 * Pick which SteamGridDB game an app pulls artwork from.
 *
 * The automatic match is by Steam appid, which is right nearly always and wrong in exactly the
 * cases that matter most: regional re-releases, remasters sharing a name, and every non-Steam
 * shortcut, whose appid SteamGridDB has never heard of.
 *
 * The search runs in Rust — the API key never reaches this window.
 */
import { useEffect, useRef, useState } from 'react';
import { api, asUiError, type GameMatch, type UiError } from '../api';
import { ErrorNote, Spinner } from '../components';
import { FocusScope, useFocusItem } from '../focus';

export function GameSearchModal({
  appId,
  gameName,
  current,
  onPicked,
  onClose,
}: {
  appId: number;
  gameName: string;
  current: GameMatch | null;
  onPicked: (game: GameMatch | null) => void;
  onClose: () => void;
}) {
  const [term, setTerm] = useState(gameName);
  const [results, setResults] = useState<GameMatch[] | null>(null);
  const [error, setError] = useState<UiError | null>(null);
  const [busy, setBusy] = useState(false);
  // Bumped on every request; a response whose id is stale is discarded. Without this a slow
  // early query can land after a fast later one and show results for a term already deleted.
  const generation = useRef(0);

  useEffect(() => {
    const query = term.trim();
    if (!query) {
      setResults(null);
      return undefined;
    }
    const mine = ++generation.current;
    // Debounced: the user is typing, and every keystroke is a network round trip otherwise.
    const timer = setTimeout(() => {
      setBusy(true);
      api
        .searchGames(query)
        .then((found) => {
          if (generation.current === mine) {
            setResults(found);
            setError(null);
          }
        })
        .catch((e: unknown) => {
          if (generation.current === mine) setError(asUiError(e));
        })
        .finally(() => {
          if (generation.current === mine) setBusy(false);
        });
    }, 250);
    return () => clearTimeout(timer);
  }, [term]);

  async function choose(game: GameMatch | null) {
    try {
      // The name goes with the id: it is the only chance to capture it, and it is what stops
      // the override reading as "SteamGridDB game #17830" next time this game is opened.
      await api.setGameOverride(appId, game?.id ?? null, game?.name ?? null);
      onPicked(game);
    } catch (e: unknown) {
      setError(asUiError(e));
    }
  }

  // Escape closes this now. It had none before — only the backdrop click and the Close button,
  // so the one key every dialog on the platform responds to did nothing here.
  return (
    <FocusScope name="game-search" onBack={onClose}>
      <div
        className="modal-backdrop"
        role="presentation"
        onClick={(e) => {
          if (e.target === e.currentTarget) onClose();
        }}
      >
        <div
          className="modal"
          role="dialog"
          aria-modal="true"
          aria-label="Choose a SteamGridDB game"
        >
          <div className="modal-head">
            <h2>Which game on SteamGridDB?</h2>
            <CloseButton onClick={onClose} />
          </div>

          <p className="hint">
            {current
              ? `Currently using “${current.name}”.`
              : 'SteamGridDB has no match for this game.'}
          </p>

          <SearchBox value={term} onChange={setTerm} />

          {error && <ErrorNote error={error} />}
          {busy && <Spinner label="Searching…" />}

          <ul className="matches">
            {/* Always offered, so an override is never a one-way door. */}
            <Match row={0} onSelect={() => void choose(null)}>
              <span className="match-name">Match automatically</span>
              <span className="match-meta">By Steam ID {appId}</span>
            </Match>
            {results?.map((game, i) => (
              <Match key={game.id} row={i + 1} onSelect={() => void choose(game)}>
                <span className="match-name">{game.name}</span>
                <span className="match-meta">
                  #{game.id}
                  {game.verified && ' · verified'}
                  {game.types.length > 0 && ` · ${game.types.join(', ')}`}
                </span>
              </Match>
            ))}
          </ul>

          {results !== null && results.length === 0 && !busy && (
            <p className="hint">Nothing found for “{term.trim()}”.</p>
          )}
        </div>
      </div>
    </FocusScope>
  );
}

function CloseButton({ onClick }: { onClick: () => void }) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('head', 0, 0);
  return (
    <button
      ref={ref}
      type="button"
      className={`ghost${focused ? ' focused' : ''}`}
      onClick={onClick}
    >
      Close
    </button>
  );
}

function SearchBox({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('search', 0, 0);
  return (
    <input
      ref={ref}
      type="search"
      className={`search${focused ? ' focused' : ''}`}
      placeholder="Search SteamGridDB…"
      value={value}
      autoFocus
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/**
 * One candidate game. The list lives inside `.modal`, which is the app's only inner scroller
 * (`max-height: 80vh; overflow-y: auto`) — `scrollIntoView({block:'nearest'})` scrolls that
 * rather than the page, which is what makes a long result list navigable.
 */
function Match({
  row,
  onSelect,
  children,
}: {
  row: number;
  onSelect: () => void;
  children: React.ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('matches', row, 0);
  return (
    <li>
      <button
        ref={ref}
        type="button"
        className={`match${focused ? ' focused' : ''}`}
        onClick={onSelect}
      >
        {children}
      </button>
    </li>
  );
}
