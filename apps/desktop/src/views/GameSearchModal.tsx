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

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-modal="true" aria-label="Choose a SteamGridDB game">
        <div className="modal-head">
          <h2>Which game on SteamGridDB?</h2>
          <button type="button" className="ghost" onClick={onClose}>
            Close
          </button>
        </div>

        <p className="hint">
          {current
            ? `Currently using “${current.name}”.`
            : 'SteamGridDB has no match for this game.'}
        </p>

        <input
          type="search"
          className="search"
          placeholder="Search SteamGridDB…"
          value={term}
          autoFocus
          onChange={(e) => setTerm(e.target.value)}
        />

        {error && <ErrorNote error={error} />}
        {busy && <Spinner label="Searching…" />}

        <ul className="matches">
          {/* Always offered, so an override is never a one-way door. */}
          <li>
            <button type="button" className="match" onClick={() => void choose(null)}>
              <span className="match-name">Match automatically</span>
              <span className="match-meta">By Steam ID {appId}</span>
            </button>
          </li>
          {results?.map((game) => (
            <li key={game.id}>
              <button type="button" className="match" onClick={() => void choose(game)}>
                <span className="match-name">{game.name}</span>
                <span className="match-meta">
                  #{game.id}
                  {game.verified && ' · verified'}
                  {game.types.length > 0 && ` · ${game.types.join(', ')}`}
                </span>
              </button>
            </li>
          ))}
        </ul>

        {results !== null && results.length === 0 && !busy && (
          <p className="hint">Nothing found for “{term.trim()}”.</p>
        )}
      </div>
    </div>
  );
}
