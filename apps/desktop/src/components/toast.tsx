/**
 * Transient confirmations.
 *
 * A subsystem rather than a small piece: it owns a context, a provider, and the timers that keep
 * the CSS animation and the removal on one clock.
 */
import { createContext, useCallback, useContext, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { asUiError } from '../api';

// -- toasts ---------------------------------------------------------------------------------

type ToastKind = 'ok' | 'info' | 'bad';

interface NewToast {
  kind: ToastKind;
  message: string;
  /** The second line — what to do about it. Only failures usually have one. */
  action?: string | null;
}

interface Toast extends NewToast {
  id: number;
  life: number;
}

/** Long enough to read a sentence without being in the way. */
const TOAST_LIFE = 4000;
/** Failures get longer: they are unexpected, so they are read from a standing start. */
const TOAST_LIFE_BAD = 7000;

const ToastContext = createContext<(t: NewToast) => void>(() => undefined);

/**
 * Transient confirmations, bottom-centre.
 *
 * **Not every message belongs here.** A toast is right when the user has just *done*
 * something and wants acknowledgement — applied artwork, reset a slot. It is wrong when the
 * message *is* the state of the view: the library's load failure renders instead of the list, so
 * fading it out would leave an empty screen with no explanation and nothing to retry. Those stay
 * as an inline {@link ErrorNote}.
 *
 * The rule: **if dismissing the message would leave the user with no idea what to do next, it
 * must not dismiss itself.**
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  // A counter rather than a timestamp or a random: two toasts raised in the same tick must not
  // collide, and React keys must be stable across renders.
  const nextId = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const notify = useCallback(
    (t: NewToast) => {
      const id = nextId.current++;
      const life = t.kind === 'bad' ? TOAST_LIFE_BAD : TOAST_LIFE;
      setToasts((prev) => [...prev, { ...t, id, life }]);
      setTimeout(() => dismiss(id), life);
    },
    [dismiss],
  );

  return (
    <ToastContext.Provider value={notify}>
      {children}
      {/* `aria-live` on the container, not the toast: a live region has to exist before the
          content arrives or a screen reader never announces it. */}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`toast toast-${t.kind}`}
            // The CSS animation fades in *and* out across exactly this span, so one timer drives
            // both the visuals and the removal. Two timers would drift apart.
            style={{ '--toast-life': `${t.life}ms` } as CSSProperties}
            onClick={() => dismiss(t.id)}
            title="Dismiss"
          >
            <span className="toast-message">{t.message}</span>
            {t.action && <span className="toast-action">{t.action}</span>}
          </button>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

/** Raise a transient message. See {@link ToastProvider} for when not to. */
export function useToast() {
  return useContext(ToastContext);
}

/** A {@link UiError} as a toast, for failures that do not stop the view working. */
export function useErrorToast() {
  const notify = useToast();
  return useCallback(
    (e: unknown) => {
      const ui = asUiError(e);
      notify({ kind: 'bad', message: ui.message, action: ui.action });
    },
    [notify],
  );
}
