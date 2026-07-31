/** Small shared pieces. Kept together because none of them is big enough to earn a file. */
import type { ReactNode } from 'react';
import type { UiError } from './api';

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
