/** Small shared pieces. Kept together because none of them is big enough to earn a file. */
import { useEffect, useState, type ReactNode } from 'react';
import type { UiError } from './api';

/**
 * An image with fallbacks, tried in order until one loads.
 *
 * Artwork comes from up to three places — the user's custom art, Steam's local cache, and
 * Steam's CDN — and which of them exist varies per game. Rather than ask the backend to decide,
 * the ladder is walked in the browser, where a failed load is already observable.
 *
 * Two details make this correct rather than merely plausible:
 *
 * - The `index >= sources.length` terminator. Without an explicit end, the last `onError`
 *   re-renders the same failing `src` and the browser retries it forever.
 * - `key={sources[index]}`. React reuses a DOM node when only `src` changes, and a node that has
 *   already errored can keep its error state, so the next rung never gets a real attempt.
 */
export function ArtImage({
  sources,
  alt,
  fallback,
}: {
  sources: string[];
  alt: string;
  fallback: ReactNode;
}) {
  const [index, setIndex] = useState(0);
  const ladder = sources.join('|');

  // A different game (or asset type) means a different ladder, which has to restart from the
  // top — otherwise a card scrolled into a position that previously failed stays blank.
  useEffect(() => setIndex(0), [ladder]);

  if (index >= sources.length) return <>{fallback}</>;
  return (
    <img
      key={sources[index]}
      src={sources[index]}
      alt={alt}
      loading="lazy"
      onError={() => setIndex((i) => i + 1)}
    />
  );
}

/**
 * An error the user can act on.
 *
 * The `action` line is the point of the whole error design — most failures here are
 * environmental (Steam closed, no key, port taken), and for those, what to do next is more
 * useful than what went wrong.
 */
export function ErrorNote({ error, onRetry }: { error: UiError; onRetry?: () => void }) {
  return (
    <div className={`note ${error.kind === 'no_api_key' ? 'note-info' : 'note-bad'}`}>
      <p className="note-message">{error.message}</p>
      {error.action && <p className="note-action">{error.action}</p>}
      {onRetry && (
        <button type="button" className="ghost" onClick={onRetry}>
          Try again
        </button>
      )}
    </div>
  );
}

export function Spinner({ label }: { label: string }) {
  return (
    <div className="spinner" role="status">
      <span className="dot" />
      {label}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty">{children}</div>;
}

/** Content-warning chips. Shown because a user filtering for them wants to see which is which. */
export function Flags({ asset }: { asset: { nsfw: boolean; humor: boolean; epilepsy: boolean } }) {
  const flags = [
    asset.nsfw && 'Adult',
    asset.humor && 'Humor',
    asset.epilepsy && 'Epilepsy',
  ].filter(Boolean) as string[];
  if (flags.length === 0) return null;
  return (
    <span className="flags">
      {flags.map((f) => (
        <span key={f} className="flag">
          {f}
        </span>
      ))}
    </span>
  );
}
